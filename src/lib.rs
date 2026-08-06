use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyBytes};
use std::collections::HashMap;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex as StdMutex, OnceLock};
static DB_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn get_db_rt() -> &'static tokio::runtime::Runtime {
    DB_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create DB runtime")
    })
}
use std::thread;
use std::time::Duration;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use notify::{RecursiveMode, Watcher};
use serde_json::json;
use tokio::sync::{oneshot, Semaphore, Mutex as TokioMutex};
use futures_util::{StreamExt, SinkExt};
use sha1::{Sha1, Digest};
use base64::{Engine as _, engine::general_purpose};

const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Segment {
    Literal(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone, Debug, PartialEq)]
enum ParamType {
    String,
    Int,
    Float,
    Bool,
}

struct DependencyMeta {
    name: String,
    func: Py<PyAny>,
    _is_async: bool,
    is_generator: bool,
    use_cache: bool,
    id: isize,
}

impl Clone for DependencyMeta {
    fn clone(&self) -> Self {
        Python::with_gil(|py| DependencyMeta {
            name: self.name.clone(),
            func: self.func.clone_ref(py),
            _is_async: self._is_async,
            is_generator: self.is_generator,
            use_cache: self.use_cache,
            id: self.id,
        })
    }
}

struct RouteEntry {
    method: String,
    original_path: String,
    segments: Vec<Segment>,
    handler: Py<PyAny>,
    is_async: bool,
    pydantic_model: Option<Py<PyAny>>,
    pydantic_param_name: Option<String>,
    request_schema_json: Option<String>,
    request_param_name: Option<String>,
    background_task_param_name: Option<String>,
    websocket_param_name: Option<String>,
    is_websocket: bool,
    dependencies: Vec<DependencyMeta>,
    param_names: Vec<String>,
    param_types: HashMap<String, ParamType>,
    required_params: Vec<String>,
    response_model: Option<Py<PyAny>>,
    tags: Vec<String>,
    summary: Option<String>,
    description: Option<String>,
}

type Routes   = Arc<StdMutex<Vec<RouteEntry>>>;
type Handlers = Arc<StdMutex<Vec<(Py<PyAny>, bool)>>>;

struct ToolEntry     { name: String, _description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
struct PromptEntry   { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

type Tools     = Arc<StdMutex<Vec<ToolEntry>>>;
type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
type Prompts   = Arc<StdMutex<Vec<PromptEntry>>>;

#[derive(Clone)]
struct NativeRouteEntry {
    method: String,
    _original_path: String,
    segments: Vec<Segment>,
    body: String,
    status_code: u16,
    content_type: String,
}

type NativeRoutes = Arc<StdMutex<Vec<NativeRouteEntry>>>;

fn match_native_route(routes: &[NativeRouteEntry], method: &str, path: &str) -> Option<NativeRouteEntry> {
    let req_segs = path_segments(path);
    for r in routes.iter() {
        if r.method != method {
            continue;
        }
        let has_wildcard = r.segments.last().map(|s| matches!(s, Segment::Wildcard(_))).unwrap_or(false);
        if !has_wildcard && r.segments.len() != req_segs.len() {
            continue;
        }
        if has_wildcard && req_segs.len() < r.segments.len().saturating_sub(1) {
            continue;
        }
        let mut ok = true;
        let seg_len = r.segments.len();
        for (i, seg) in r.segments.iter().enumerate() {
            if i == seg_len - 1 {
                if let Segment::Wildcard(_) = seg {
                    break;
                }
            }
            if i >= req_segs.len() {
                ok = false;
                break;
            }
            match seg {
                Segment::Literal(l) => {
                    if l != req_segs[i] { ok = false; break; }
                }
                Segment::Param(_) => {}
                Segment::Wildcard(_) => break,
            }
        }
        if ok { return Some(r.clone()); }
    }
    None
}

// ---------------------------------------------------------------------------
// Routing helpers
// ---------------------------------------------------------------------------

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn parse_pattern(path: &str) -> Vec<Segment> {
    path_segments(path)
        .into_iter()
        .map(|s| {
            if s.starts_with('{') && s.ends_with('}') {
                let inner = &s[1..s.len() - 1];
                if inner.contains(":path") {
                    let clean = inner.split(':').next().unwrap_or(inner).to_string();
                    Segment::Wildcard(clean)
                } else if inner.starts_with('*') {
                    Segment::Wildcard(inner[1..].to_string())
                } else {
                    Segment::Param(inner.to_string())
                }
            } else if s.starts_with('*') {
                Segment::Wildcard(s[1..].to_string())
            } else {
                Segment::Literal(s.to_string())
            }
        })
        .collect()
}

fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
    let req_segs = path_segments(path);
    for (idx, r) in routes.iter().enumerate() {
        if r.method != method {
            continue;
        }
        let has_wildcard = r.segments.last().map(|s| matches!(s, Segment::Wildcard(_))).unwrap_or(false);
        if !has_wildcard && r.segments.len() != req_segs.len() {
            continue;
        }
        if has_wildcard && req_segs.len() < r.segments.len().saturating_sub(1) {
            continue;
        }

        let mut params = HashMap::new();
        let mut ok = true;
        let seg_len = r.segments.len();

        for (i, seg) in r.segments.iter().enumerate() {
            if i == seg_len - 1 {
                if let Segment::Wildcard(name) = seg {
                    let rest = if i < req_segs.len() { req_segs[i..].join("/") } else { String::new() };
                    params.insert(name.clone(), rest);
                    break;
                }
            }
            if i >= req_segs.len() {
                ok = false;
                break;
            }
            let val = req_segs[i];
            match seg {
                Segment::Literal(l) => {
                    if l != val { ok = false; break; }
                }
                Segment::Param(name) => {
                    params.insert(name.clone(), (*val).to_string());
                }
                Segment::Wildcard(name) => {
                    let rest = req_segs[i..].join("/");
                    params.insert(name.clone(), rest);
                    break;
                }
            }
        }
        if ok { return Some((idx, params)); }
    }
    None
}

fn parse_query(query: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(q) = query {
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let mut it = pair.splitn(2, '=');
            let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
            let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
            map.insert(k, v);
        }
    }
    map
}

// ---------------------------------------------------------------------------
// WebSocket / OpenAPI helpers
// ---------------------------------------------------------------------------

fn compute_websocket_accept(key: &str) -> String {
    let mut sha1 = Sha1::new();
    sha1.update(key.as_bytes());
    sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    general_purpose::STANDARD.encode(sha1.finalize())
}

fn generate_openapi(routes: &[RouteEntry]) -> String {
    let mut paths = serde_json::Map::new();
    let mut components_schemas = serde_json::Map::new();
    let mut security_schemes = serde_json::Map::new();

    for r in routes {
        if r.is_websocket { continue; }
        let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });

        let mut tags_vec = r.tags.clone();
        if tags_vec.is_empty() {
            let first_seg = r.original_path.trim_start_matches('/').split('/').next().unwrap_or("");
            if !first_seg.is_empty() && !first_seg.starts_with('{') {
                tags_vec.push(first_seg.to_string());
            }
        }

        if !tags_vec.is_empty() {
            method_obj["tags"] = json!(tags_vec);
        }
        if let Some(ref sum) = r.summary {
            method_obj["summary"] = json!(sum);
        }
        if let Some(ref desc) = r.description {
            method_obj["description"] = json!(desc);
        }

        // Security schemes collection
        let mut route_security = Vec::new();
        Python::with_gil(|py| {
            let mut stack = Vec::new();
            for dep in &r.dependencies {
                stack.push(dep.func.bind(py).clone());
            }

            let mut visited = std::collections::HashSet::new();
            let inspect = py.import_bound("inspect").ok();

            while let Some(bound_fn) = stack.pop() {
                let dep_ptr = bound_fn.as_ptr() as usize;
                if !visited.insert(dep_ptr) { continue; }

                let type_name = bound_fn.get_type().name().map(|n| n.to_string()).unwrap_or_default();

                if type_name == "HTTPBearer" || bound_fn.getattr("bearerFormat").is_ok() {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "HTTPBearer".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "http", "scheme": "bearer" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "OAuth2AuthorizationCodeBearer" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "OAuth2AuthorizationCodeBearer".to_string());
                    let auth_url = bound_fn.getattr("authorizationUrl").and_then(|s| s.extract::<String>()).unwrap_or_default();
                    let token_url = bound_fn.getattr("tokenUrl").and_then(|s| s.extract::<String>()).unwrap_or_default();
                    security_schemes.insert(s_name.clone(), json!({
                        "type": "oauth2",
                        "flows": { "authorizationCode": { "authorizationUrl": auth_url, "tokenUrl": token_url, "scopes": {} } }
                    }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "OAuth2PasswordBearer" || bound_fn.getattr("tokenUrl").is_ok() {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "OAuth2PasswordBearer".to_string());
                    let token_url = bound_fn.getattr("tokenUrl").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "/token".to_string());
                    security_schemes.insert(s_name.clone(), json!({
                        "type": "oauth2",
                        "flows": { "password": { "tokenUrl": token_url, "scopes": {} } }
                    }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "APIKeyHeader" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "APIKeyHeader".to_string());
                    let key_name = bound_fn.getattr("name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "X-API-Key".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "apiKey", "name": key_name, "in": "header" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "APIKeyQuery" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "APIKeyQuery".to_string());
                    let key_name = bound_fn.getattr("name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "api_key".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "apiKey", "name": key_name, "in": "query" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "HTTPBasic" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "HTTPBasic".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "http", "scheme": "basic" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "OAuth2AuthorizationCodeBearer" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "OAuth2AuthorizationCodeBearer".to_string());
                    let auth_url = bound_fn.getattr("authorizationUrl").and_then(|s| s.extract::<String>()).unwrap_or_default();
                    let token_url = bound_fn.getattr("tokenUrl").and_then(|s| s.extract::<String>()).unwrap_or_default();
                    security_schemes.insert(s_name.clone(), json!({
                        "type": "oauth2",
                        "flows": { "authorizationCode": { "authorizationUrl": auth_url, "tokenUrl": token_url, "scopes": {} } }
                    }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "APIKeyCookie" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "APIKeyCookie".to_string());
                    let key_name = bound_fn.getattr("name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "session".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "apiKey", "name": key_name, "in": "cookie" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "HTTPDigest" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "HTTPDigest".to_string());
                    security_schemes.insert(s_name.clone(), json!({ "type": "http", "scheme": "digest" }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if type_name == "OpenIdConnect" {
                    let s_name = bound_fn.getattr("scheme_name").and_then(|s| s.extract::<String>()).unwrap_or_else(|_| "OpenIdConnect".to_string());
                    let openid_url = bound_fn.getattr("openIdConnectUrl").and_then(|s| s.extract::<String>()).unwrap_or_default();
                    security_schemes.insert(s_name.clone(), json!({ "type": "openIdConnect", "openIdConnectUrl": openid_url }));
                    if !route_security.iter().any(|s: &serde_json::Value| s.get(&s_name).is_some()) {
                        route_security.push(json!({ s_name: [] }));
                    }
                } else if let Some(ref insp) = inspect {
                    if let Ok(sig) = insp.call_method1("signature", (&bound_fn,)) {
                        if let Ok(params) = sig.getattr("parameters") {
                            if let Ok(values) = params.call_method0("values") {
                                if let Ok(iter) = values.iter() {
                                    for p_res in iter {
                                        if let Ok(p) = p_res {
                                            if let Ok(d_val) = p.getattr("default") {
                                                let is_depends = d_val
                                                    .getattr("__class__")
                                                    .and_then(|cls| cls.getattr("__name__"))
                                                    .and_then(|n| n.extract::<String>())
                                                    .map(|n| n == "Depends")
                                                    .unwrap_or(false);
                                                if is_depends {
                                                    if let Ok(sub_fn) = d_val.getattr("dependency") {
                                                        if !sub_fn.is_none() {
                                                            stack.push(sub_fn);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        if !route_security.is_empty() {
            method_obj["security"] = json!(route_security);
        }

        // Parameters (path and query)
        let mut parameters = Vec::new();

        for seg in &r.segments {
            if let Segment::Param(ref name) = seg {
                let ptype = match r.param_types.get(name) {
                    Some(ParamType::Int) => "integer",
                    Some(ParamType::Float) => "number",
                    Some(ParamType::Bool) => "boolean",
                    _ => "string",
                };
                parameters.push(json!({
                    "name": name,
                    "in": "path",
                    "required": true,
                    "schema": { "type": ptype }
                }));
            }
        }

        for req_p in &r.required_params {
            let is_path_param = r.segments.iter().any(|s| matches!(s, Segment::Param(p_name) if p_name == req_p));
            let is_pydantic_param = r.pydantic_param_name.as_ref().map(|p| p == req_p).unwrap_or(false);
            let is_req_param = r.request_param_name.as_ref().map(|p| p == req_p).unwrap_or(false) || req_p == "req" || req_p == "request";
            let is_bg_param = r.background_task_param_name.as_ref().map(|p| p == req_p).unwrap_or(false);

            if !is_path_param && !is_pydantic_param && !is_req_param && !is_bg_param {
                let ptype = match r.param_types.get(req_p) {
                    Some(ParamType::Int) => "integer",
                    Some(ParamType::Float) => "number",
                    Some(ParamType::Bool) => "boolean",
                    _ => "string",
                };
                parameters.push(json!({
                    "name": req_p,
                    "in": "query",
                    "required": true,
                    "schema": { "type": ptype }
                }));
            }
        }

        if !parameters.is_empty() {
            method_obj["parameters"] = serde_json::Value::Array(parameters);
        }

        if matches!(r.method.as_str(), "POST" | "PUT" | "PATCH") {
            if r.original_path.contains("upload") {
                method_obj["requestBody"] = json!({
                    "required": true,
                    "content": { "multipart/form-data": { "schema": { "type": "object", "properties": {
                        "document":    { "type": "string", "format": "binary" },
                        "description": { "type": "string" }
                    }}}}
                });
            } else if let Some(ref schema_str) = r.request_schema_json {
                if let Ok(schema_val) = serde_json::from_str::<serde_json::Value>(schema_str) {
                    let schema_str_clean = schema_val.to_string().replace("#/$defs/", "#/components/schemas/");
                    if let Ok(mut clean_val) = serde_json::from_str::<serde_json::Value>(&schema_str_clean) {
                        if let Some(defs) = clean_val.as_object_mut().and_then(|obj| obj.remove("$defs")) {
                            if let Some(defs_map) = defs.as_object() {
                                for (k, v) in defs_map {
                                    let mut sub_schema = v.clone();
                                    if let Some(sub_obj) = sub_schema.as_object_mut() {
                                        sub_obj.remove("additionalProperties");
                                    }
                                    components_schemas.insert(k.clone(), sub_schema);
                                }
                            }
                        }
                        if let Some(obj) = clean_val.as_object_mut() {
                            obj.remove("additionalProperties");
                        }
                        let model_name = clean_val.get("title")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string());

                        if let Some(name) = model_name {
                            components_schemas.insert(name.clone(), clean_val);
                            method_obj["requestBody"] = json!({
                                "required": true,
                                "content": {
                                    "application/json": {
                                        "schema": { "$ref": format!("#/components/schemas/{}", name) }
                                    }
                                }
                            });
                        } else {
                            method_obj["requestBody"] = json!({
                                "required": true,
                                "content": { "application/json": { "schema": clean_val } }
                            });
                        }
                    } else {
                        method_obj["requestBody"] = json!({
                            "required": true,
                            "content": { "application/json": { "schema": schema_val } }
                        });
                    }
                }
            } else if r.pydantic_param_name.is_some() || r.request_param_name.is_some() {
                method_obj["requestBody"] = json!({
                    "required": true,
                    "content": { "application/json": { "schema": { "type": "object", "example": {} } } }
                });
            }
        }
        let method_lower = r.method.to_lowercase();
        if let Some(path_item) = paths.get_mut(&r.original_path) {
            path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
        } else {
            paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
        }
    }

    let mut doc = json!({
        "openapi": "3.0.0",
        "info": { "title": "RustAPI", "version": "0.1.0" },
        "paths": paths
    });

    let mut components = serde_json::Map::new();
    if !components_schemas.is_empty() {
        components.insert("schemas".to_string(), json!(components_schemas));
    }
    if !security_schemes.is_empty() {
        components.insert("securitySchemes".to_string(), json!(security_schemes));
    }

    if !components.is_empty() {
        doc["components"] = serde_json::Value::Object(components);
    }

    serde_json::to_string(&doc).unwrap()
}

fn swagger_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Swagger UI</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: '/openapi.json',
        dom_id: '#swagger-ui',
        deepLinking: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIBundle.SwaggerUIStandalonePreset
        ],
        layout: "StandaloneLayout"
      });
    };
  </script>
</body>
</html>"#.to_string()
}

fn redoc_html() -> String {
    r#"<!DOCTYPE html>
<html>
<head>
<title>ReDoc</title>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1">
<link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
<style>body { margin: 0; padding: 0; }</style>
</head>
<body>
<noscript>ReDoc requires Javascript to function.</noscript>
<redoc spec-url="/openapi.json"></redoc>
<script src="https://cdn.jsdelivr.net/npm/redoc@2/bundles/redoc.standalone.js"> </script>
</body>
</html>"#.to_string()
}

// ---------------------------------------------------------------------------
// Python-exposed types
// ---------------------------------------------------------------------------

#[pyclass(name = "WebSocket")]
struct PyWebSocket {
    stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
    rt: tokio::runtime::Handle,
}

#[pymethods]
impl PyWebSocket {
    fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
        let stream = self.stream.clone();
        let rt = self.rt.clone();
        py.allow_threads(move || {
            rt.block_on(async move {
                let mut lock = stream.lock().await;
                while let Some(msg) = lock.next().await {
                    if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                        return Ok(text.to_string());
                    }
                }
                Err(pyo3::exceptions::PyConnectionAbortedError::new_err("Connection closed"))
            })
        })
    }

    fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        let stream = self.stream.clone();
        let rt = self.rt.clone();
        py.allow_threads(move || {
            rt.block_on(async move {
                let mut lock = stream.lock().await;
                lock.send(tokio_tungstenite::tungstenite::Message::Text(text.into()))
                    .await
                    .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
                Ok(())
            })
        })
    }
}

#[pyclass(name = "UploadFile")]
#[derive(Clone)]
struct PyUploadFile {
    #[pyo3(get)] filename: String,
    #[pyo3(get)] content_type: String,
    file_data: Vec<u8>,
}

#[pymethods]
impl PyUploadFile {
    #[new]
    #[pyo3(signature = (filename="".to_string(), content_type="".to_string(), file_data=Vec::new()))]
    fn new(filename: String, content_type: String, file_data: Vec<u8>) -> Self {
        PyUploadFile { filename, content_type, file_data }
    }

    #[pyo3(signature = (_size=-1))]
    fn read(&self, py: Python<'_>, _size: i64) -> PyResult<PyObject> {
        let bytes_obj = PyBytes::new_bound(py, &self.file_data);
        if let Ok(py_module) = py.import_bound("rustapi.uploads") {
            if let Ok(awaitable_cls) = py_module.getattr("AwaitableBytes") {
                if let Ok(res) = awaitable_cls.call1((&bytes_obj,)) {
                    return Ok(res.unbind());
                }
            }
        }
        Ok(bytes_obj.into())
    }

    #[pyo3(signature = (_offset=0))]
    fn seek(&self, py: Python<'_>, _offset: i64) -> PyResult<PyObject> {
        if let Ok(py_module) = py.import_bound("rustapi.uploads") {
            if let Ok(awaitable_cls) = py_module.getattr("AwaitableInt") {
                if let Ok(res) = awaitable_cls.call1((0,)) {
                    return Ok(res.unbind());
                }
            }
        }
        Ok(0.to_object(py))
    }

    fn close(&self, py: Python<'_>) -> PyResult<PyObject> {
        if let Ok(py_module) = py.import_bound("rustapi.uploads") {
            if let Ok(awaitable_cls) = py_module.getattr("AwaitableNone") {
                if let Ok(res) = awaitable_cls.call0() {
                    return Ok(res.unbind());
                }
            }
        }
        Ok(py.None())
    }

    #[getter]
    fn file(&self, py: Python<'_>) -> PyResult<PyObject> {
        let io = py.import_bound("io")?;
        let bytes_obj = PyBytes::new_bound(py, &self.file_data);
        io.call_method1("BytesIO", (bytes_obj,)).map(|b| b.unbind())
    }
}

fn serde_to_pyobject(py: Python<'_>, val: &serde_json::Value) -> PyResult<PyObject> {
    match val {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.to_object(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_object(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_object(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.to_object(py)),
        serde_json::Value::Array(arr) => {
            let py_list = pyo3::types::PyList::empty_bound(py);
            for v in arr {
                py_list.append(serde_to_pyobject(py, v)?)?;
            }
            Ok(py_list.into_any().unbind())
        }
        serde_json::Value::Object(map) => {
            let py_dict = pyo3::types::PyDict::new_bound(py);
            for (k, v) in map {
                py_dict.set_item(k, serde_to_pyobject(py, v)?)?;
            }
            Ok(py_dict.into_any().unbind())
        }
    }
}

#[pyclass]
struct PyRequest {
    #[pyo3(get)] method: String,
    #[pyo3(get)] path: String,
    #[pyo3(get)] path_params: HashMap<String, String>,
    #[pyo3(get)] query_params: HashMap<String, String>,
    #[pyo3(get)] headers: HashMap<String, String>,
    #[pyo3(get)] cookies: HashMap<String, String>,
    #[pyo3(get)] form: HashMap<String, String>,
    #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>,
    #[pyo3(get)] body: String,
}

#[pymethods]
impl PyRequest {
    #[new]
    #[pyo3(signature = (method, path, path_params=None, query_params=None, headers=None, cookies=None, form=None, files=None, body=None))]
    fn new(
        method: String,
        path: String,
        path_params: Option<HashMap<String, String>>,
        query_params: Option<HashMap<String, String>>,
        headers: Option<HashMap<String, String>>,
        cookies: Option<HashMap<String, String>>,
        form: Option<HashMap<String, String>>,
        files: Option<HashMap<String, Vec<PyUploadFile>>>,
        body: Option<String>,
    ) -> Self {
        PyRequest {
            method,
            path,
            path_params: path_params.unwrap_or_default(),
            query_params: query_params.unwrap_or_default(),
            headers: headers.unwrap_or_default(),
            cookies: cookies.unwrap_or_default(),
            form: form.unwrap_or_default(),
            files: files.unwrap_or_default(),
            body: body.unwrap_or_default(),
        }
    }

    #[pyo3(signature = ())]
    fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        let trimmed = self.body.trim();
        let raw_obj = if trimmed.is_empty() {
            pyo3::types::PyDict::new_bound(py).into_any().unbind()
        } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&self.body) {
            serde_to_pyobject(py, &val)?
        } else {
            let py_json = py.import_bound("json")?;
            py_json.call_method1("loads", (&self.body,)).map(|b| b.unbind())?
        };

        if let Ok(uploads_mod) = py.import_bound("rustapi.uploads") {
            if let Ok(cls) = uploads_mod.getattr("AwaitableDict") {
                if let Ok(awaitable_dict) = cls.call1((raw_obj.bind(py),)) {
                    return Ok(awaitable_dict.unbind().into());
                }
            }
        }
        Ok(raw_obj)
    }
}

#[pyclass(name = "Response", subclass)]
struct PyResponse {
    #[pyo3(get)] content: PyObject,
    #[pyo3(get)] status_code: u16,
    #[pyo3(get)] headers: HashMap<String, String>,
}

impl Clone for PyResponse {
    fn clone(&self) -> Self {
        Python::with_gil(|py| PyResponse {
            content: self.content.clone_ref(py),
            status_code: self.status_code,
            headers: self.headers.clone(),
        })
    }
}

#[pymethods]
impl PyResponse {
    #[new]
    #[pyo3(signature = (content, status_code=200, headers=None))]
    fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self {
        PyResponse { content, status_code, headers: headers.unwrap_or_default() }
    }
}

#[pyclass(name = "StreamingResponse")]
struct PyStreamingResponse {
    #[pyo3(get)] content: PyObject,
    #[pyo3(get)] status_code: u16,
    #[pyo3(get)] headers: HashMap<String, String>,
}

impl Clone for PyStreamingResponse {
    fn clone(&self) -> Self {
        Python::with_gil(|py| PyStreamingResponse {
            content: self.content.clone_ref(py),
            status_code: self.status_code,
            headers: self.headers.clone(),
        })
    }
}

#[pymethods]
impl PyStreamingResponse {
    #[new]
    #[pyo3(signature = (content, status_code=200, headers=None, media_type=None))]
    fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>, media_type: Option<String>) -> Self {
        let mut h = headers.unwrap_or_default();
        if let Some(mt) = media_type {
            h.entry("content-type".to_string()).or_insert(mt);
        }
        PyStreamingResponse { content, status_code, headers: h }
    }
}

// ---------------------------------------------------------------------------
// Async/coroutine bridge
// ---------------------------------------------------------------------------

#[pyclass]
struct CoroCallback {
    tx: StdMutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>>,
}

#[pymethods]
impl CoroCallback {
    #[pyo3(signature = (result, error))]
    fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            if error.is_none(py) { let _ = tx.send(Ok(result)); }
            else { let _ = tx.send(Err(error)); }
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming response builder — single canonical implementation
// ---------------------------------------------------------------------------

/// Converts a `PyStreamingResponse` into a chunked `HyperResponse<Body>`.
/// This is the single source of truth; the old duplicate blocks are gone.
fn build_streaming_response(
    content: PyObject,
    status: u16,
    headers: HashMap<String, String>,
) -> HyperResponse<Body> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<hyper::body::Bytes, Infallible>>(32);

    tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let Ok(builtins) = py.import_bound("builtins") else { return; };
            let Ok(iterator) = builtins.call_method1("iter", (&content,)) else { return; };
            let Ok(next_fn) = builtins.getattr("next") else { return; };
            loop {
                match next_fn.call1((&iterator,)) {
                    Ok(item) => {
                        let chunk_bytes: Vec<u8> =
                            if let Ok(s) = item.downcast::<pyo3::types::PyString>() {
                                s.to_string().into_bytes()
                            } else if let Ok(b) = item.downcast::<PyBytes>() {
                                b.as_bytes().to_vec()
                            } else {
                                item.to_string().into_bytes()
                            };
                        if tx.blocking_send(Ok(hyper::body::Bytes::from(chunk_bytes))).is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // StopIteration
                }
            }
        });
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });
    let stream_body = Body::wrap_stream::<_, hyper::body::Bytes, Infallible>(stream);

    let mut builder = HyperResponse::builder().status(status);
    let mut final_headers = headers;
    if !final_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) {
        final_headers.insert("content-type".to_string(), "text/plain; charset=utf-8".to_string());
    }
    for (k, v) in final_headers {
        builder = builder.header(&k, &v);
    }
    builder.body(stream_body).unwrap()
}

// ---------------------------------------------------------------------------
// Python handler executor
// ---------------------------------------------------------------------------

async fn execute_python_handler(
    exec_res: PyResult<PyObject>,
    is_async: bool,
    serializer: &PyObject,
    filter_response: &PyObject,
    schedule_coro: &PyObject,
    raw_string: bool,
    response_model: Option<&PyObject>,
) -> (u16, String, HashMap<String, String>) {
    // Await coroutine if the handler is async.
    let py_result: PyResult<PyObject> = if is_async {
        match exec_res {
            Ok(coro) => {
                let (tx, rx) = oneshot::channel();
                let spawn_res = Python::with_gil(|py| -> PyResult<()> {
                    let cb = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) })?;
                    schedule_coro.bind(py).call1((coro, cb))?;
                    Ok(())
                });
                if let Err(e) = spawn_res {
                    Err(e)
                } else {
                    match rx.await {
                        Ok(Ok(res))      => Ok(res),
                        Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))),
                        Err(_)           => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))),
                    }
                }
            }
            Err(e) => Err(e),
        }
    } else {
        exec_res
    };

    Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
        match py_result {
            Ok(py_obj) => {
                // Handle explicit Response wrapper.
                if let Ok(resp_ref) = py_obj.extract::<PyRef<PyResponse>>(py) {
                    let status = resp_ref.status_code;
                    let headers = resp_ref.headers.clone();
                    let is_raw = headers.get("content-type")
                        .or_else(|| headers.get("Content-Type"))
                        .map(|ct| ct.contains("application/json") || ct.contains("text/"))
                        .unwrap_or(false);
                    let content_obj = if let Some(model) = response_model {
                        filter_response.bind(py).call1((&resp_ref.content, model))
                            .map(|b| b.unbind())
                            .unwrap_or_else(|_| resp_ref.content.clone_ref(py))
                    } else {
                        resp_ref.content.clone_ref(py)
                    };
                    let body_str = serialize_value(py, &content_obj, serializer, is_raw || raw_string);
                    return (status, body_str, headers);
                }
                // Plain return value.
                let filtered_obj = if let Some(model) = response_model {
                    filter_response.bind(py).call1((&py_obj, model))
                        .map(|b| b.unbind())
                        .unwrap_or_else(|_| py_obj.clone_ref(py))
                } else {
                    py_obj
                };
                let body_str = serialize_value(py, &filtered_obj, serializer, raw_string);
                (200, body_str, HashMap::new())
            }
            Err(err) => {
                // First try direct attribute inspection on the Python exception object (e.g., HTTPException)
                let custom_err = Python::with_gil(|py| -> Option<(u16, String)> {
                    let val = err.value_bound(py);
                    if let Ok(code_obj) = val.getattr("status_code") {
                        if let Ok(code) = code_obj.extract::<u16>() {
                            let detail_str = if let Ok(d) = val.getattr("detail") {
                                d.to_string()
                            } else {
                                String::new()
                            };
                            return Some((code, detail_str));
                        }
                    }
                    None
                });

                if let Some((code, detail)) = custom_err {
                    let clean = detail.replace('"', "'");
                    return (code, format!(r#"{{"detail":"{}"}}"#, clean), HashMap::new());
                }

                let err_str = err.to_string();
                // Fallback: search for "422: ..." or "N: ..." in error string
                for part in err_str.split(": ") {
                    if let Ok(code) = part.trim().parse::<u16>() {
                        if (100..=599).contains(&code) {
                            let clean = err_str.rsplit(": ").next().unwrap_or(&err_str).replace('"', "'");
                            return (code, format!(r#"{{"detail":"{}"}}"#, clean), HashMap::new());
                        }
                    }
                }
                (500, format!(r#"{{"detail":"{}"}}"#, err_str.replace('"', "'")), HashMap::new())
            }
        }
    })
}

/// Serialize a Python object to a string, handling `None` and plain strings
/// specially when `raw_string` is true.
fn serialize_value(py: Python<'_>, obj: &PyObject, serializer: &PyObject, raw_string: bool) -> String {
    if raw_string {
        if obj.is_none(py) { return String::new(); }
        if let Ok(s) = obj.downcast_bound::<pyo3::types::PyString>(py) {
            return s.to_string();
        }
    }
    serializer.bind(py).call1((obj,)).unwrap().extract().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

#[pyclass(name = "Route")]
struct PyRoute {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    methods: Vec<String>,
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    summary: Option<String>,
    #[pyo3(get)]
    description: Option<String>,
    #[pyo3(get)]
    tags: Vec<String>,
    #[pyo3(get)]
    dependencies: Vec<PyObject>,
    #[pyo3(get)]
    endpoint: PyObject,
}

#[pyclass(name = "Engine", subclass)]
struct Engine {
    routes: Routes,
    serializer: PyObject,
    filter_response_fn: PyObject,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    schema_fn: PyObject,
    schedule_coro_fn: PyObject,
    startup_handlers: Handlers,
    shutdown_handlers: Handlers,
    native_routes: NativeRoutes,
    #[pyo3(get, set)]
    dependency_overrides: PyObject,
    #[pyo3(get)]
    db: Option<Py<PyDatabase>>,
}

#[allow(non_local_definitions)]
#[pymethods]
impl Engine {
    #[getter]
    fn routes(&self, py: Python<'_>) -> PyResult<Vec<PyRoute>> {
        let guard = self.routes.lock().unwrap();
        let mut list = Vec::new();
        for r in guard.iter() {
            let name = r.handler.getattr(py, "__name__")
                .and_then(|n| n.extract::<String>(py))
                .unwrap_or_else(|_| "handler".to_string());
            let mut deps = Vec::new();
            for d in &r.dependencies {
                deps.push(d.func.clone_ref(py));
            }
            list.push(PyRoute {
                path: r.original_path.clone(),
                methods: vec![r.method.clone()],
                name,
                summary: r.summary.clone(),
                description: r.description.clone(),
                tags: r.tags.clone(),
                dependencies: deps,
                endpoint: r.handler.clone_ref(py),
            });
        }
        Ok(list)
    }
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let python_code = r#"
import asyncio, inspect, json, threading
_engine_loop = asyncio.new_event_loop()
def _start_engine_loop():
    asyncio.set_event_loop(_engine_loop)
    _engine_loop.run_forever()
threading.Thread(target=_start_engine_loop, daemon=True).start()
def _schedule_coro(coro, callback):
    def done_cb(fut):
        try: callback(fut.result(), None)
        except Exception as e: callback(None, e)
    fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
    fut.add_done_callback(done_cb)
def _serialize_response(val):
    return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
def _filter_response(val, response_model):
    if response_model is None or val is None:
        return val
    try:
        from pydantic import TypeAdapter
        if isinstance(val, list) and hasattr(response_model, "model_validate"):
            adapter = TypeAdapter(list[response_model])
        else:
            adapter = TypeAdapter(response_model)
        validated = adapter.validate_python(val)
        return adapter.dump_python(validated, mode="json")
    except Exception:
        if hasattr(response_model, "model_validate"):
            if isinstance(val, list):
                return [response_model.model_validate(i).model_dump(mode="json") for i in val]
            return response_model.model_validate(val).model_dump(mode="json")
        return val
def _schema_from_signature(func):
    sig = inspect.signature(func)
    props = {name: {"type": "string"} for name in sig.parameters}
    return {"type": "object", "properties": props}
"#;
        let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
        let overrides = pyo3::types::PyDict::new_bound(py).unbind().into();
        Ok(Engine {
            routes:               Arc::new(StdMutex::new(Vec::new())),
            native_routes:        Arc::new(StdMutex::new(Vec::new())),
            serializer:           module.getattr("_serialize_response")?.into(),
            filter_response_fn:   module.getattr("_filter_response")?.into(),
            schedule_coro_fn:     module.getattr("_schedule_coro")?.into(),
            schema_fn:            module.getattr("_schema_from_signature")?.into(),
            tools:                Arc::new(StdMutex::new(Vec::new())),
            resources:            Arc::new(StdMutex::new(Vec::new())),
            prompts:              Arc::new(StdMutex::new(Vec::new())),
            startup_handlers:      Arc::new(StdMutex::new(Vec::new())),
            shutdown_handlers:     Arc::new(StdMutex::new(Vec::new())),
            dependency_overrides: overrides,
            db: None,
        })
    }

    // -- Route decorators --------------------------------------------------
    #[pyo3(signature = (method, path, is_ws, response_model=None, kwargs=None))]
    fn make_route_decorator(&self, method: String, path: String, is_ws: bool, response_model: Option<Py<PyAny>>, kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator {
        let mut tags = Vec::new();
        let mut summary = None;
        let mut description = None;

        if let Some(dict) = kwargs {
            if let Ok(Some(t_val)) = dict.get_item("tags") {
                if let Ok(t_list) = t_val.extract::<Vec<String>>() {
                    tags = t_list;
                }
            }
            if let Ok(Some(s_val)) = dict.get_item("summary") {
                if let Ok(s_str) = s_val.extract::<String>() {
                    summary = Some(s_str);
                }
            }
            if let Ok(Some(d_val)) = dict.get_item("description") {
                if let Ok(d_str) = d_val.extract::<String>() {
                    description = Some(d_str);
                }
            }
        }

        RouteDecorator {
            routes: self.routes.clone(),
            method,
            path,
            is_ws,
            response_model,
            tags,
            summary,
            description,
        }
    }

    #[pyo3(signature = (path, response_model=None, **_kwargs))]
    fn get    (&self, path: String, response_model: Option<Py<PyAny>>, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("GET".into(), path, false, response_model, _kwargs) }
    #[pyo3(signature = (path, response_model=None, **_kwargs))]
    fn post   (&self, path: String, response_model: Option<Py<PyAny>>, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("POST".into(), path, false, response_model, _kwargs) }
    #[pyo3(signature = (path, response_model=None, **_kwargs))]
    fn put    (&self, path: String, response_model: Option<Py<PyAny>>, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("PUT".into(), path, false, response_model, _kwargs) }
    #[pyo3(signature = (path, response_model=None, **_kwargs))]
    fn delete (&self, path: String, response_model: Option<Py<PyAny>>, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("DELETE".into(), path, false, response_model, _kwargs) }
    #[pyo3(signature = (path, response_model=None, **_kwargs))]
    fn patch  (&self, path: String, response_model: Option<Py<PyAny>>, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("PATCH".into(), path, false, response_model, _kwargs) }
    #[pyo3(signature = (path, **_kwargs))]
    fn websocket(&self, path: String, _kwargs: Option<&Bound<'_, PyDict>>) -> RouteDecorator { self.make_route_decorator("GET".into(), path, true, None, _kwargs) }

    #[pyo3(signature = (path, body, method="GET", status_code=200, content_type="application/json"))]
    fn add_native_route(&self, path: String, body: String, method: &str, status_code: u16, content_type: &str) {
        let segments = parse_pattern(&path);
        let entry = NativeRouteEntry {
            method: method.to_uppercase(),
            _original_path: path,
            segments,
            body,
            status_code,
            content_type: content_type.to_string(),
        };
        self.native_routes.lock().unwrap().push(entry);
    }

    #[pyo3(signature = (method, path, query_string, headers, body))]
    fn dispatch_request(
        &self,
        py: Python<'_>,
        method: String,
        path: String,
        query_string: String,
        headers: HashMap<String, String>,
        body: String,
    ) -> PyResult<PyObject> {
        let asyncio = py.import_bound("asyncio")?;
        let loop_obj = asyncio.call_method0("get_running_loop").or_else(|_| asyncio.call_method0("get_event_loop"))?;
        let fut = loop_obj.call_method0("create_future")?;
        let loop_py: PyObject = loop_obj.clone().unbind().into();
        let fut_py: PyObject = fut.clone().unbind().into();
        let fut_py_cb = fut_py.clone_ref(py);
        let loop_py_cb = loop_py.clone_ref(py);

        let routes = self.routes.clone();
        let native_routes = self.native_routes.clone();
        let serializer = Arc::new(self.serializer.clone_ref(py));
        let filter_response = Arc::new(self.filter_response_fn.clone_ref(py));
        let schedule_coro = Arc::new(self.schedule_coro_fn.clone_ref(py));
        let gil_sem = Arc::new(Semaphore::new(100));
        let dependency_overrides = Arc::new(self.dependency_overrides.clone_ref(py));

        get_db_rt().spawn(async move {
            let matched_native = {
                let guard = native_routes.lock().unwrap();
                match_native_route(&guard, &method, &path)
            };
            if let Some(entry) = matched_native {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), entry.content_type);
                Python::with_gil(|py| {
                    let res_tuple: PyObject = (entry.status_code, entry.body, h).into_py(py);
                    if let Ok(set_res) = fut_py_cb.bind(py).getattr("set_result") {
                        let _ = loop_py_cb.bind(py).call_method1("call_soon_threadsafe", (set_res, res_tuple));
                    }
                });
                return;
            }

            let query_params = parse_query(if query_string.is_empty() { None } else { Some(&query_string) });
            let mut cookies_map = HashMap::new();
            if let Some(c_hdr) = headers.get("cookie").or_else(|| headers.get("Cookie")) {
                for pair in c_hdr.split(';') {
                    let mut parts = pair.trim().splitn(2, '=');
                    if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) {
                        cookies_map.insert(ck.to_string(), cv.to_string());
                    }
                }
            }

            let matched = {
                let guard = routes.lock().unwrap();
                match_route(&guard, &method, &path)
            };

            if let Some((idx, path_params)) = matched {
                let resp_res = handle_route(
                    idx, path_params, method, path, body, headers, cookies_map, query_params,
                    HashMap::new(), HashMap::new(), &routes, &serializer, &filter_response,
                    &schedule_coro, &gil_sem, &dependency_overrides,
                ).await;

                match resp_res {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let mut resp_headers = HashMap::new();
                        for (k, v) in resp.headers() {
                            resp_headers.insert(k.as_str().to_string(), v.to_str().unwrap_or("").to_string());
                        }
                        let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap_or_default().to_vec();
                        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                        Python::with_gil(|py| {
                            let res_tuple: PyObject = (status, body_str, resp_headers).into_py(py);
                            if let Ok(set_res) = fut_py_cb.bind(py).getattr("set_result") {
                                let _ = loop_py_cb.bind(py).call_method1("call_soon_threadsafe", (set_res, res_tuple));
                            }
                        });
                    }
                    Err(err_msg) => {
                        Python::with_gil(|py| {
                            let mut h = HashMap::new();
                            h.insert("content-type".to_string(), "application/json".to_string());
                            let err_json = format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'"));
                            let res_tuple: PyObject = (500, err_json, h).into_py(py);
                            if let Ok(set_res) = fut_py_cb.bind(py).getattr("set_result") {
                                let _ = loop_py_cb.bind(py).call_method1("call_soon_threadsafe", (set_res, res_tuple));
                            }
                        });
                    }
                }
            } else {
                Python::with_gil(|py| {
                    let mut h = HashMap::new();
                    h.insert("content-type".to_string(), "application/json".to_string());
                    let res_tuple: PyObject = (404, r#"{"detail":"Not Found"}"#.to_string(), h).into_py(py);
                    if let Ok(set_res) = fut_py_cb.bind(py).getattr("set_result") {
                        let _ = loop_py_cb.bind(py).call_method1("call_soon_threadsafe", (set_res, res_tuple));
                    }
                });
            }
        });

        Ok(fut_py)
    }

    // -- Rust-Native Database Engine ---------------------------------------
    #[pyo3(signature = (url))]
    fn connect_db(&mut self, py: Python<'_>, url: String) -> PyResult<Py<PyDatabase>> {
        let u = url.clone();

        let (sqlite, pg) = py.allow_threads(move || {
            let conn_str = if u == "sqlite::memory:" || u == "sqlite://:memory:" {
                "sqlite://file:memdb1?mode=memory&cache=shared".to_string()
            } else {
                u.clone()
            };
            get_db_rt().block_on(async move {
                if conn_str.starts_with("sqlite") {
                    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous, SqlitePoolOptions};
                    use std::str::FromStr;
                    use std::time::Duration;

                    let opts = SqliteConnectOptions::from_str(&conn_str)
                        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
                        .journal_mode(SqliteJournalMode::Wal)
                        .synchronous(SqliteSynchronous::Normal)
                        .busy_timeout(Duration::from_secs(10))
                        .create_if_missing(true);

                    let pool = SqlitePoolOptions::new()
                        .max_connections(50)
                        .connect_with(opts)
                        .await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok::<_, PyErr>((Some(pool), None))
                } else if conn_str.starts_with("postgres") {
                    let pool = sqlx::postgres::PgPoolOptions::new()
                        .max_connections(50)
                        .connect(&conn_str)
                        .await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok::<_, PyErr>((None, Some(pool)))
                } else {
                    Err(pyo3::exceptions::PyValueError::new_err(format!("Unsupported database URL scheme in '{}'", conn_str)))
                }
            })
        })?;

        let db_obj = Py::new(py, PyDatabase {
            sqlite_pool: sqlite,
            pg_pool: pg,
        })?;

        self.db = Some(db_obj.clone_ref(py));
        Ok(db_obj)
    }

    /// Mount a sub-router, supporting all HTTP methods.
    #[pyo3(signature = (router, prefix = "".to_string(), **_kwargs))]
    fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let router_prefix: String = router.getattr(py, "prefix").and_then(|p| p.extract(py)).unwrap_or_default();
        let router_tags: Vec<String> = router.getattr(py, "tags").and_then(|t| t.extract(py)).unwrap_or_default();

        let inc_tags: Vec<String> = if let Some(dict) = _kwargs {
            dict.get_item("tags").ok().flatten().and_then(|t| t.extract().ok()).unwrap_or_default()
        } else {
            Vec::new()
        };

        let base_prefix = format!("{}{}", prefix, router_prefix);

        for item_res in router.getattr(py, "routes")?.bind(py).iter()? {
            let item = item_res?;
            let len = item.len()?;
            let method: String = item.get_item(0)?.extract()?;
            let path: String = item.get_item(1)?.extract()?;
            let func: Py<PyAny> = item.get_item(2)?.extract()?;
            let response_model: Option<Py<PyAny>> = if len > 3 { item.get_item(3)?.extract().ok() } else { None };
            let route_kwargs: Option<Bound<'_, PyDict>> = if len > 4 { item.get_item(4)?.extract().ok() } else { None };

            let raw_path = format!("{}{}", base_prefix, path).replace("//", "/");
            let full_path = if raw_path.starts_with('/') { raw_path } else { format!("/{}", raw_path) };

            let route_tags: Vec<String> = route_kwargs.as_ref()
                .and_then(|dict| dict.get_item("tags").ok().flatten())
                .and_then(|t| t.extract().ok())
                .unwrap_or_default();

            let mut merged_tags = Vec::new();
            for t in inc_tags.iter().chain(router_tags.iter()).chain(route_tags.iter()) {
                if !merged_tags.contains(t) {
                    merged_tags.push(t.clone());
                }
            }

            let kw = PyDict::new_bound(py);
            if !merged_tags.is_empty() {
                kw.set_item("tags", merged_tags)?;
            }
            if let Some(ref r_kw) = route_kwargs {
                if let Ok(Some(s)) = r_kw.get_item("summary") {
                    kw.set_item("summary", s)?;
                }
                if let Ok(Some(d)) = r_kw.get_item("description") {
                    kw.set_item("description", d)?;
                }
            }

            match method.as_str() {
                "GET"    => { self.get(full_path, response_model, Some(&kw)).__call__(py, func)?; }
                "POST"   => { self.post(full_path, response_model, Some(&kw)).__call__(py, func)?; }
                "PUT"    => { self.put(full_path, response_model, Some(&kw)).__call__(py, func)?; }
                "DELETE" => { self.delete(full_path, response_model, Some(&kw)).__call__(py, func)?; }
                "PATCH"  => { self.patch(full_path, response_model, Some(&kw)).__call__(py, func)?; }
                "WS"     => { self.websocket(full_path, Some(&kw)).__call__(py, func)?; }
                other    => eprintln!("include_router: unsupported method '{}'", other),
            };
        }
        Ok(())
    }

    // -- Lifecycle events --------------------------------------------------
    #[pyo3(signature = (event_type))]
    fn on_event(&self, event_type: String) -> PyResult<EventDecorator> {
        match event_type.as_str() {
            "startup"  => Ok(EventDecorator { handlers: self.startup_handlers.clone() }),
            "shutdown" => Ok(EventDecorator { handlers: self.shutdown_handlers.clone() }),
            _          => Err(pyo3::exceptions::PyValueError::new_err("Invalid event type")),
        }
    }

    // -- MCP decorators ----------------------------------------------------
    #[pyo3(signature = (name=None, description=None))]
    fn tool(&self, py: Python<'_>, name: Option<String>, description: Option<String>) -> ToolDecorator {
        ToolDecorator { tools: self.tools.clone(), schema_fn: self.schema_fn.clone_ref(py), name, description }
    }

    #[pyo3(signature = (uri, mime_type=None))]
    fn resource(&self, uri: String, mime_type: Option<String>) -> ResourceDecorator {
        ResourceDecorator { resources: self.resources.clone(), uri, mime_type }
    }

    #[pyo3(signature = (name=None, description=None))]
    fn prompt(&self, name: Option<String>, description: Option<String>) -> PromptDecorator {
        PromptDecorator { prompts: self.prompts.clone(), name, description }
    }

    // -- Server entry-point ------------------------------------------------
    #[pyo3(signature = (host = "127.0.0.1".to_string(), port = 8000, reload = false, workers = 1))]
    fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
        let is_worker  = std::env::var("RUSTAPI_WORKER").is_ok();
        let safe_workers = workers.max(1);

        // Supervisor mode: spawn child workers and optionally watch for file changes.
        if (reload || safe_workers > 1) && !is_worker {
            let sys = py.import_bound("sys")?;
            let executable: String = sys.getattr("executable")?.extract()?;
            let argv: Vec<String>  = sys.getattr("argv")?.extract()?;

            if reload {
                eprintln!("INFO:     Will watch for file changes in '.'");
            }

            let exit_result: bool = py.allow_threads(move || {
                struct ChildGuard(Vec<std::process::Child>);
                impl Drop for ChildGuard {
                    fn drop(&mut self) {
                        for c in &mut self.0 {
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                    }
                }

                let spawn_children = || -> Vec<std::process::Child> {
                    (0..safe_workers)
                        .map(|i| Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap())
                        .collect()
                };

                let mut guard = ChildGuard(spawn_children());
                let (tx, rx) = std::sync::mpsc::channel();
                let _watcher = if reload {
                    let mut w = notify::recommended_watcher(tx).unwrap();
                    w.watch(Path::new("."), RecursiveMode::Recursive).unwrap();
                    Some(w)
                } else {
                    None
                };

                let interrupted = loop {
                    if reload {
                        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
                            if event.paths.iter().any(|p| p.extension().map_or(false, |e| e == "py")) {
                                if let Some(changed_path) = event.paths.iter().find(|p| p.extension().map_or(false, |e| e == "py")) {
                                    eprintln!("INFO:     Stat of file changed: {}. Reloading...", changed_path.display());
                                }
                                for mut c in guard.0.drain(..) { let _ = c.kill(); let _ = c.wait(); }
                                guard.0 = spawn_children();
                                continue;
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(250));
                    }
                    if Python::with_gil(|py| py.check_signals().is_err()) {
                        break true;
                    }
                };
                interrupted
            });

            if exit_result {
                if let Err(err) = py.check_signals() {
                    if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { return Ok(()); } else { return Err(err); }
                }
            }
            return Ok(());
        }

        // Worker mode: start the Tokio HTTP server.
        let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
        let socket = socket2::Socket::new(
            if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 },
            socket2::Type::STREAM, None,
        ).unwrap();
        socket.set_reuse_address(true).unwrap();
        #[cfg(unix)] socket.set_reuse_port(true).unwrap();
        socket.bind(&addr.into()).unwrap();
        socket.listen(1024).unwrap();
        let std_listener: std::net::TcpListener = socket.into();
        std_listener.set_nonblocking(true).unwrap();

        eprintln!("INFO:     Started server process [{}]", std::process::id());
        eprintln!("INFO:     RustAPI server running on http://{host}:{port} (Press CTRL+C to quit)");

        let routes                = self.routes.clone();
        let native_routes         = self.native_routes.clone();
        let tools                 = self.tools.clone();
        let resources             = self.resources.clone();
        let prompts               = self.prompts.clone();
        let serializer_arc        = Arc::new(self.serializer.clone_ref(py));
        let filter_response_arc   = Arc::new(self.filter_response_fn.clone_ref(py));
        let schedule_coro_arc     = Arc::new(self.schedule_coro_fn.clone_ref(py));
        let dependency_overrides_arc = Arc::new(self.dependency_overrides.clone_ref(py));
        let num_cpus              = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let gil_semaphore         = Arc::new(Semaphore::new(num_cpus * 2));
        let startup_handlers      = self.startup_handlers.clone();
        let shutdown_handlers     = self.shutdown_handlers.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let mut shutdown_tx       = Some(shutdown_tx);
        let (done_tx, done_rx)    = mpsc::channel::<()>();

        let server_handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .max_blocking_threads(num_cpus * 4)
                .build()
                .unwrap();

            rt.block_on(async move {
                let sc2  = schedule_coro_arc.clone();
                let sem2 = gil_semaphore.clone();

                // Run startup handlers.
                run_lifecycle_handlers(&startup_handlers, &sc2, &sem2).await;

                // Build and serve.
                let make_svc = make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
                    let remote_addr = conn.remote_addr();
                    let (r, nr, t, res, p, s, fr, sc, sem, do_arc) = (
                        routes.clone(), native_routes.clone(), tools.clone(), resources.clone(), prompts.clone(),
                        serializer_arc.clone(), filter_response_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone(),
                        dependency_overrides_arc.clone(),
                    );
                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| {
                            let start_time = std::time::Instant::now();
                            let method = req.method().to_string();
                            let uri_str = req.uri().path_and_query().map(|pq| pq.as_str().to_string()).unwrap_or_else(|| req.uri().path().to_string());
                            let r2 = r.clone();
                            let nr2 = nr.clone();
                            let s2 = s.clone();
                            let fr2 = fr.clone();
                            let sc2 = sc.clone();
                            let t2 = t.clone();
                            let res2 = res.clone();
                            let p2 = p.clone();
                            let sem2 = sem.clone();
                            let do_arc2 = do_arc.clone();

                            async move {
                                let resp = handle(req, r2, nr2, s2, fr2, sc2, t2, res2, p2, sem2, do_arc2).await;
                                let duration = start_time.elapsed();
                                let status_code = resp.as_ref().map(|res| res.status().as_u16()).unwrap_or(500);
                                let duration_ms = duration.as_secs_f64() * 1000.0;
                                if std::env::var("RUSTAPI_LOG").ok().as_deref() != Some("0") {
                                    println!(
                                        "INFO:     {} - \"{} {} HTTP/1.1\" {} - {:.2}ms",
                                        remote_addr, method, uri_str, status_code, duration_ms
                                    );
                                }
                                resp
                            }
                        }))
                    }
                });
                let server   = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
                let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
                
                if let Err(e) = graceful.await {
                    eprintln!("Server error: {e}");
                }

                // Run shutdown handlers.
                run_lifecycle_handlers(&shutdown_handlers, &sc2, &sem2).await;
            });

            let _ = done_tx.send(());
        });

        // Poll Python signals on the main thread so Ctrl-C works.
        let pending_err = py.allow_threads(move || {
            let mut captured_err = None;
            loop {
                if done_rx.try_recv().is_ok() {
                    break;
                }
                if captured_err.is_none() {
                    if let Err(err) = Python::with_gil(|py| py.check_signals()) {
                        if let Some(tx) = shutdown_tx.take() {
                            let _ = tx.send(());
                        }
                        if !is_worker {
                            eprintln!("\nINFO:     Shutting down RustAPI server...");
                        }
                        captured_err = Some(err);
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
            let _ = server_handle.join();
            captured_err
        });

        match pending_err {
            Some(err) => Python::with_gil(|py| {
                if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) }
            }),
            None => Ok(()),
        }
    }
}

/// Runs startup or shutdown lifecycle handlers sequentially.
async fn run_lifecycle_handlers(
    handlers: &Handlers,
    schedule_coro: &Arc<PyObject>,
    gil_sem: &Arc<Semaphore>,
) {
    let list = Python::with_gil(|py| {
        handlers.lock().unwrap().iter().map(|(h, a)| (h.clone_ref(py), *a)).collect::<Vec<_>>()
    });
    for (handler, is_async) in list {
        if is_async {
            let coro: Result<PyObject, PyErr> =
                Python::with_gil(|py| handler.bind(py).call0().map(|v| v.into()));
            if let Ok(c) = coro {
                let (tx, rx) = oneshot::channel();
                let sc = schedule_coro.clone();
                Python::with_gil(|py| {
                    if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                        let _ = sc.bind(py).call1((c, cb));
                    }
                });
                let _ = rx.await;
            }
        } else {
            let sem  = gil_sem.clone();
            tokio::task::spawn_blocking(move || {
                let _permit = sem.try_acquire();
                Python::with_gil(|py| { let _ = handler.bind(py).call0(); });
            })
            .await
            .unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Main request handler
// ---------------------------------------------------------------------------

async fn handle(
    mut req: HyperRequest<Body>,
    routes: Routes,
    native_routes: NativeRoutes,
    serializer: Arc<PyObject>,
    filter_response: Arc<PyObject>,
    schedule_coro: Arc<PyObject>,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    gil_sem: Arc<Semaphore>,
    dependency_overrides: Arc<PyObject>,
) -> Result<HyperResponse<Body>, Infallible> {
    let method      = req.method().to_string();
    let path        = req.uri().path().to_string();

    let matched_native = {
        let guard = native_routes.lock().unwrap();
        match_native_route(&guard, &method, &path)
    };
    if let Some(entry) = matched_native {
        let resp = HyperResponse::builder()
            .status(entry.status_code)
            .header("content-type", entry.content_type)
            .body(Body::from(entry.body))
            .unwrap();
        return Ok(resp);
    }

    let query_params = parse_query(req.uri().query());

    // Collect headers and cookies.
    let mut headers_map = HashMap::<String, String>::new();
    let mut cookies_map = HashMap::<String, String>::new();
    for (k, v) in req.headers() {
        let key = k.as_str().to_string();
        let val = v.to_str().unwrap_or("").to_string();
        if key.eq_ignore_ascii_case("cookie") {
            for pair in val.split(';') {
                let mut parts = pair.trim().splitn(2, '=');
                if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) {
                    cookies_map.insert(ck.to_string(), cv.to_string());
                }
            }
        }
        headers_map.insert(key, val);
    }

    // -----------------------------------------------------------------------
    // WebSocket upgrade path
    // -----------------------------------------------------------------------
    let is_ws_upgrade = req.headers()
        .get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws_upgrade {
        let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
        if let Some((idx, _)) = matched {
            let (handler, ws_param_name) = Python::with_gil(|py| {
                let guard = routes.lock().unwrap();
                let entry = &guard[idx];
                (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
            });
            if let Some(ws_name) = ws_param_name {
                if let Some(ws_key) = req.headers().get("sec-websocket-key").and_then(|v| v.to_str().ok()) {
                    let accept_key    = compute_websocket_accept(ws_key);
                    let sc_ws         = schedule_coro.clone();
                    tokio::spawn(async move {
                        if let Ok(upgraded) = hyper::upgrade::on(&mut req).await {
                            let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
                                upgraded,
                                tokio_tungstenite::tungstenite::protocol::Role::Server,
                                None,
                            ).await;
                            let ws_arc   = Arc::new(TokioMutex::new(ws_stream));
                            let ws_pyobj = Python::with_gil(|py| {
                                Py::new(py, PyWebSocket { stream: ws_arc, rt: tokio::runtime::Handle::current() })
                                    .unwrap()
                                    .into_any()
                            });
                            let coro = Python::with_gil(|py| {
                                let kw = pyo3::types::PyDict::new_bound(py);
                                let _  = kw.set_item(&ws_name, ws_pyobj.bind(py));
                                handler.bind(py).call((), Some(&kw)).map(|b| b.unbind()).ok()
                            });
                            if let Some(c) = coro {
                                let (tx, rx) = oneshot::channel();
                                Python::with_gil(|py| {
                                    if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                                        let _ = sc_ws.bind(py).call1((c, cb));
                                    }
                                });
                                let _ = rx.await;
                            }
                        }
                    });
                    return Ok(
                        HyperResponse::builder()
                            .status(StatusCode::SWITCHING_PROTOCOLS)
                            .header(hyper::header::UPGRADE, "websocket")
                            .header(hyper::header::CONNECTION, "upgrade")
                            .header("sec-websocket-accept", accept_key)
                            .body(Body::empty())
                            .unwrap(),
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Body / multipart parsing
    // -----------------------------------------------------------------------
    let mut form_map  = HashMap::<String, String>::new();
    let mut files_map = HashMap::<String, Vec<PyUploadFile>>::new();
    let mut body_bytes = Vec::<u8>::new();
    let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");

    if let Ok(boundary) = multer::parse_boundary(content_type) {
        let mut multipart = multer::Multipart::new(req.into_body(), boundary);
        while let Ok(Some(field)) = multipart.next_field().await {
            let name      = field.name().unwrap_or("").to_string();
            let file_name = field.file_name().map(|s| s.to_string());
            let c_type    = field.content_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());
            if let Some(fn_str) = file_name {
                let data = field.bytes().await.unwrap_or_default().to_vec();
                files_map.entry(name).or_insert_with(Vec::new).push(
                    PyUploadFile { filename: fn_str, content_type: c_type, file_data: data }
                );
            } else {
                form_map.insert(name, field.text().await.unwrap_or_default());
            }
        }
    } else {
        use futures_util::StreamExt;
        let mut body_stream = req.into_body();
        while let Some(chunk) = body_stream.next().await {
            if let Ok(data) = chunk {
                if body_bytes.len() + data.len() > MAX_PAYLOAD_SIZE {
                    let mut h = HashMap::new();
                    h.insert("Content-Type".to_string(), "application/json".to_string());
                    let resp = HyperResponse::builder()
                        .status(413)
                        .body(Body::from(r#"{"detail":"Payload Too Large"}"#))
                        .unwrap();
                    return Ok(resp);
                }
                body_bytes.extend_from_slice(&data);
            }
        }
    }
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // -----------------------------------------------------------------------
    // Internal built-in routes & route matching
    // -----------------------------------------------------------------------

    if method == "OPTIONS" {
        let mut h = HashMap::new();
        h.insert("Access-Control-Allow-Origin".to_string(), "*".to_string());
        h.insert("Access-Control-Allow-Methods".to_string(), "GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD".to_string());
        h.insert("Access-Control-Allow-Headers".to_string(), "*".to_string());
        h.insert("Access-Control-Allow-Credentials".to_string(), "true".to_string());
        let mut builder = HyperResponse::builder().status(200);
        for (k, v) in h { builder = builder.header(&k, &v); }
        return Ok(builder.body(Body::empty()).unwrap());
    }

    let (status, resp_body, mut resp_headers) = if method == "GET" && path == "/docs" {
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        (200u16, swagger_html(), h)

    } else if method == "GET" && path == "/redoc" {
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        (200u16, redoc_html(), h)

    } else if method == "GET" && path == "/openapi.json" {
        let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        (200u16, spec, h)

    } else if method == "POST" && path == "/mcp" {
        let result = handle_mcp(&body, &tools, &resources, &prompts, &serializer, &schedule_coro, &gil_sem).await;
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        match result {
            Ok(body_str) => (200u16, body_str, h),
            Err(resp)    => return Ok(resp),
        }

    } else {
        // User-defined route.
        match { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) } {
            Some((idx, path_params)) => {
                let result = handle_route(
                    idx, path_params, method, path, body,
                    headers_map, cookies_map, query_params,
                    form_map, files_map,
                    &routes, &serializer, &filter_response, &schedule_coro, &gil_sem, &dependency_overrides,
                ).await;
                match result {
                    Ok(resp)      => return Ok(resp),
                    Err(body_str) => {
                        let mut h = HashMap::new();
                        h.insert("Content-Type".to_string(), "application/json".to_string());
                        (500u16, body_str, h)
                    }
                }
            }
            None => {
                let mut h = HashMap::new();
                h.insert("Content-Type".to_string(), "application/json".to_string());
                (404u16, r#"{"detail":"Not Found"}"#.to_string(), h)
            }
        }
    };

    resp_headers.entry("Access-Control-Allow-Origin".to_string()).or_insert("*".to_string());
    resp_headers.entry("Access-Control-Allow-Methods".to_string()).or_insert("GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD".to_string());
    resp_headers.entry("Access-Control-Allow-Headers".to_string()).or_insert("*".to_string());

    let mut builder = HyperResponse::builder().status(status);
    for (k, v) in resp_headers { builder = builder.header(&k, &v); }
    Ok(builder.body(Body::from(resp_body)).unwrap())
}

// ---------------------------------------------------------------------------
// MCP handler
// ---------------------------------------------------------------------------

async fn handle_mcp(
    body: &str,
    tools: &Tools,
    resources: &Resources,
    prompts: &Prompts,
    serializer: &Arc<PyObject>,
    schedule_coro: &Arc<PyObject>,
    gil_sem: &Arc<Semaphore>,
) -> Result<String, HyperResponse<Body>> {
    let req_json: serde_json::Value = serde_json::from_str(body).unwrap_or(json!({}));
    let req_method = req_json["method"].as_str().unwrap_or("").to_string();
    let has_id  = req_json.get("id").is_some();
    let msg_id  = req_json["id"].clone();
    let params  = req_json.get("params").unwrap_or(&json!({})).clone();

    // Notifications (no id) → 202 with empty body.
    if !has_id {
        return Err(HyperResponse::builder().status(202).body(Body::empty()).unwrap());
    }

    let ok  = |res: serde_json::Value| json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string();
    let err = |code: i32, msg: &str|   json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string();

    let result = match req_method.as_str() {
        "initialize" => ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
            "serverInfo": { "name": "rustapi-mcp", "version": "0.1.0" }
        })),
        "ping" => ok(json!({})),

        "tools/list" => {
            let guard = tools.lock().unwrap();
            let items: Vec<_> = guard.iter()
                .map(|t| json!({ "name": t.name, "description": t._description, "inputSchema": t.schema_json }))
                .collect();
            ok(json!({ "tools": items }))
        }
        "resources/list" => {
            let guard = resources.lock().unwrap();
            let items: Vec<_> = guard.iter()
                .map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type }))
                .collect();
            ok(json!({ "resources": items }))
        }
        "resources/read" => {
            let uri = params["uri"].as_str().unwrap_or("");
            let guard = resources.lock().unwrap();
            match guard.iter().find(|r| r.uri == uri) {
                Some(res_entry) => {
                    let text = Python::with_gil(|py| {
                        res_entry.handler.bind(py).call0()
                            .map(|v| v.extract::<String>().unwrap_or_default())
                            .unwrap_or_default()
                    });
                    ok(json!({ "contents": [{ "uri": res_entry.uri, "mimeType": res_entry.mime_type, "text": text }] }))
                }
                None => err(-32602, &format!("Unknown resource: {}", uri)),
            }
        }
        "prompts/list" => {
            let guard = prompts.lock().unwrap();
            let items: Vec<_> = guard.iter()
                .map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] }))
                .collect();
            ok(json!({ "prompts": items }))
        }
        "prompts/get" => {
            let name  = params["name"].as_str().unwrap_or("");
            let topic = params.get("arguments")
                .and_then(|a| a.get("topic"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let guard = prompts.lock().unwrap();
            match guard.iter().find(|p| p.name == name) {
                Some(entry) => {
                    let text = Python::with_gil(|py| {
                        let kw = pyo3::types::PyDict::new_bound(py);
                        let _  = kw.set_item("topic", topic);
                        entry.handler.bind(py).call((), Some(&kw))
                            .map(|v| v.extract::<String>().unwrap_or_default())
                            .unwrap_or_default()
                    });
                    ok(json!({ "messages": [{ "role": "user", "content": { "type": "text", "text": text } }] }))
                }
                None => err(-32602, &format!("Unknown prompt: {}", name)),
            }
        }
        "tools/call" => {
            let name      = params["name"].as_str().unwrap_or("").to_string();
            let args_json = params["arguments"].clone();
            let tool_opt  = Python::with_gil(|py| {
                tools.lock().unwrap().iter()
                    .find(|t| t.name == name)
                    .map(|t| (t.handler.clone_ref(py), t._is_async))
            });
            match tool_opt {
                Some((handler, is_async_tool)) => {
                    let _permit = gil_sem.acquire().await.ok();
                    let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                        Python::with_gil(|py| -> PyResult<PyObject> {
                            let kw = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                            if let Ok(dict) = kw.downcast::<PyDict>() {
                                handler.bind(py).call((), Some(dict)).map(|v| v.into())
                            } else {
                                handler.bind(py).call0().map(|v| v.into())
                            }
                        })
                    }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));

                    let (t_status, content, _) =
                        execute_python_handler(exec_res, is_async_tool, serializer, serializer, schedule_coro, true, None).await;
                    ok(json!({ "content": [{ "type": "text", "text": content }], "isError": t_status >= 400 }))
                }
                None => err(-32602, &format!("Unknown tool: {}", name)),
            }
        }
        other => err(-32601, &format!("Method not found: {}", other)),
    };

    Ok(result)
}

// ---------------------------------------------------------------------------
// User-defined route handler
// ---------------------------------------------------------------------------

/// Returns `Ok(HyperResponse)` for both normal and streaming responses.
/// Returns `Err(String)` only for hard dependency/task errors before the handler runs.
async fn handle_route(
    idx: usize,
    path_params: HashMap<String, String>,
    method: String,
    path: String,
    body: String,
    headers_map: HashMap<String, String>,
    cookies_map: HashMap<String, String>,
    query_params: HashMap<String, String>,
    form_map: HashMap<String, String>,
    files_map: HashMap<String, Vec<PyUploadFile>>,
    routes: &Routes,
    serializer: &Arc<PyObject>,
    filter_response: &Arc<PyObject>,
    schedule_coro: &Arc<PyObject>,
    gil_sem: &Arc<Semaphore>,
    dependency_overrides: &Arc<PyObject>,
) -> Result<HyperResponse<Body>, String> {
    // Extract route metadata.
    let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name,
         background_task_param_name, deps, param_names, param_types, required_params, response_model) =
        Python::with_gil(|py| {
            let guard = routes.lock().unwrap();
            let e = &guard[idx];
            (
                e.handler.clone_ref(py), e.is_async,
                e.pydantic_model.as_ref().map(|m| m.clone_ref(py)),
                e.pydantic_param_name.clone(), e.request_param_name.clone(),
                e.background_task_param_name.clone(),
                e.dependencies.clone(), e.param_names.clone(), e.param_types.clone(),
                e.required_params.clone(),
                e.response_model.as_ref().map(|m| m.clone_ref(py)),
            )
        });

    // -- Dependency injection ---------------------------------------------
    let req_obj: PyObject = Python::with_gil(|py| {
        Py::new(py, PyRequest {
            method: method.clone(), path: path.clone(), path_params: path_params.clone(),
            query_params: query_params.clone(), headers: headers_map.clone(), cookies: cookies_map.clone(),
            form: form_map.clone(), files: files_map.clone(), body: body.clone(),
        }).unwrap().into_any()
    });

    let mut dependency_error_response: Option<HyperResponse<Body>> = None;
    let mut resolved_args   = HashMap::<String, PyObject>::new();
    let dep_cache_obj       = Python::with_gil(|py| pyo3::types::PyDict::new_bound(py).into_any().unbind());
    let teardown_gens       = Vec::<PyObject>::new();

    for dep in deps {
        let dep_func = Python::with_gil(|py| -> PyObject {
            if let Ok(dict) = dependency_overrides.bind(py).downcast::<pyo3::types::PyDict>() {
                if let Ok(Some(ov)) = dict.get_item(&dep.func) {
                    if !ov.is_none() {
                        return ov.unbind();
                    }
                }
            }
            dep.func.clone_ref(py)
        });

        let solve_coro = Python::with_gil(|py| -> PyResult<PyObject> {
            let resolver = py.import_bound("rustapi.resolver")?;
            let solver = resolver.getattr("solve_dependency")?;
            let ov_bound = dependency_overrides.bind(py);
            solver.call1((dep_func, &req_obj, ov_bound, &dep_cache_obj)).map(|c| c.unbind())
        });

        match solve_coro {
            Ok(coro) => {
                let (tx, rx) = oneshot::channel();
                let sc = schedule_coro.clone();
                Python::with_gil(|py| {
                    if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                        let _ = sc.bind(py).call1((coro, cb));
                    }
                });
                match rx.await {
                    Ok(Ok(res)) => {
                        resolved_args.insert(dep.name, res);
                    }
                    Ok(Err(err_obj)) => {
                        let resp = Python::with_gil(|py| -> HyperResponse<Body> {
                            let bound_err = err_obj.bind(py);
                            let status_code: u16 = bound_err.getattr("status_code")
                                .and_then(|s| s.extract())
                                .unwrap_or(500);

                            let detail_json: String = if let Ok(detail) = bound_err.getattr("detail") {
                                if let Ok(py_json) = py.import_bound("json") {
                                    if let Ok(dumps_res) = py_json.call_method1("dumps", (&detail,)) {
                                        let d_str: String = dumps_res.extract().unwrap_or_default();
                                        format!(r#"{{"detail":{}}}"#, d_str)
                                    } else {
                                        format!(r#"{{"detail":"{}"}}"#, detail.to_string().replace('"', "'"))
                                    }
                                } else {
                                    format!(r#"{{"detail":"{}"}}"#, detail.to_string().replace('"', "'"))
                                }
                            } else {
                                format!(r#"{{"detail":"{}"}}"#, bound_err.to_string().replace('"', "'"))
                            };

                            HyperResponse::builder()
                                .status(status_code)
                                .header("Content-Type", "application/json")
                                .body(Body::from(detail_json))
                                .unwrap()
                        });
                        dependency_error_response = Some(resp);
                        break;
                    }
                    Err(_) => {
                        dependency_error_response = Some(
                            HyperResponse::builder()
                                .status(500)
                                .header("Content-Type", "application/json")
                                .body(Body::from(r#"{"detail":"Asyncio task dropped"}"#))
                                .unwrap()
                        );
                        break;
                    }
                }
            }
            Err(py_err) => {
                let resp = Python::with_gil(|py| -> HyperResponse<Body> {
                    let err_val = py_err.into_value(py);
                    let bound_err = err_val.bind(py);
                    let status_code: u16 = bound_err.getattr("status_code")
                        .and_then(|s| s.extract())
                        .unwrap_or(500);

                    let detail_json: String = if let Ok(detail) = bound_err.getattr("detail") {
                        if let Ok(py_json) = py.import_bound("json") {
                            if let Ok(dumps_res) = py_json.call_method1("dumps", (&detail,)) {
                                let d_str: String = dumps_res.extract().unwrap_or_default();
                                format!(r#"{{"detail":{}}}"#, d_str)
                            } else {
                                format!(r#"{{"detail":"{}"}}"#, detail.to_string().replace('"', "'"))
                            }
                        } else {
                            format!(r#"{{"detail":"{}"}}"#, detail.to_string().replace('"', "'"))
                        }
                    } else {
                        format!(r#"{{"detail":"{}"}}"#, bound_err.to_string().replace('"', "'"))
                    };

                    HyperResponse::builder()
                        .status(status_code)
                        .header("Content-Type", "application/json")
                        .body(Body::from(detail_json))
                        .unwrap()
                });
                dependency_error_response = Some(resp);
                break;
            }
        }
    }

    if let Some(err_resp) = dependency_error_response {
        return Ok(err_resp);
    }

    // -- BackgroundTasks setup --------------------------------------------
    let bg_tasks_obj: Option<PyObject> = if background_task_param_name.is_some() {
        Python::with_gil(|py| {
            py.import_bound("rustapi.background").ok()
                .and_then(|m| m.getattr("BackgroundTasks").ok())
                .and_then(|cls| cls.call0().ok())
                .map(|i| i.into())
        })
    } else {
        None
    };
    let bg_obj_for_call   = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|o| o.clone_ref(py)));
    let bg_param_name_c   = background_task_param_name.clone();

    // -- Call the handler -------------------------------------------------
    let sem_c               = gil_sem.clone();
    let param_names_c       = param_names.clone();
    let required_params_c   = required_params.clone();
    let path_params_c       = path_params.clone();
    let query_params_c      = query_params.clone();
    let method_c            = method.clone();
    let path_c              = path.clone();
    let body_c              = body.clone();
    let headers_c           = headers_map.clone();
    let cookies_c           = cookies_map.clone();
    let form_c              = form_map.clone();
    let files_c             = files_map.clone();

    let permit = sem_c.acquire_owned().await.ok();

    let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        Python::with_gil(|py| -> PyResult<PyObject> {
            let kwargs = pyo3::types::PyDict::new_bound(py);

            // Enforce required parameters presence.
            for req_p in &required_params_c {
                if !path_params_c.contains_key(req_p) && !query_params_c.contains_key(req_p) {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!("422: Missing required parameter '{}'", req_p)));
                }
            }

            // Type-coercing parameter binder (path + query).
            let apply_params = |params: &HashMap<String, String>| -> Result<(), String> {
                for (k, v) in params {
                    if !param_names_c.contains(k) { continue; }
                    match param_types.get(k).unwrap_or(&ParamType::String) {
                        ParamType::Int => {
                            v.parse::<i64>().map(|n| kwargs.set_item(k, n).map_err(|e| e.to_string()))
                                .map_err(|_| format!("Parameter '{}' must be an integer", k))??;
                        }
                        ParamType::Float => {
                            v.parse::<f64>().map(|n| kwargs.set_item(k, n).map_err(|e| e.to_string()))
                                .map_err(|_| format!("Parameter '{}' must be a float", k))??;
                        }
                        ParamType::Bool => {
                            let b = match v.to_lowercase().as_str() {
                                "true" | "1"  => true,
                                "false" | "0" => false,
                                _ => return Err(format!("Parameter '{}' must be a boolean", k)),
                            };
                            kwargs.set_item(k, b).map_err(|e| e.to_string())?;
                        }
                        ParamType::String => {
                            kwargs.set_item(k, v).map_err(|e| e.to_string())?;
                        }
                    }
                }
                Ok(())
            };

            if let Err(e) = apply_params(&path_params_c)  { return Err(pyo3::exceptions::PyValueError::new_err(format!("422: {}", e))); }
            if let Err(e) = apply_params(&query_params_c) { return Err(pyo3::exceptions::PyValueError::new_err(format!("422: {}", e))); }

            for (k, v) in resolved_args { kwargs.set_item(k, v)?; }

            if let Some(req_name) = request_param_name {
                let req_obj = Py::new(py, PyRequest {
                    method: method_c, path: path_c, path_params: path_params_c,
                    query_params: query_params_c, headers: headers_c, cookies: cookies_c,
                    form: form_c, files: files_c, body: body_c.clone(),
                })?;
                kwargs.set_item(req_name, req_obj)?;
            }

            if let Some(ref model) = pydantic_model {
                let py_dict = if body_c.is_empty() {
                    pyo3::types::PyDict::new_bound(py).into_any()
                } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_c) {
                    serde_to_pyobject(py, &val)?.into_bound(py)
                } else {
                    py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any()
                };
                let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
                if let Some(model_name) = pydantic_param_name {
                    kwargs.set_item(model_name, instance)?;
                }
            }

            if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_c, bg_obj_for_call) {
                kwargs.set_item(bg_name, bg_obj.bind(py))?;
            }

            handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
        })
    })
    .await
    .unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

    // -- StreamingResponse fast-path (single canonical check) -------------
    let streaming_data = Python::with_gil(|py| {
        if let Ok(ref obj) = exec_res {
            if let Ok(bound) = obj.bind(py).downcast::<PyStreamingResponse>() {
                let r = bound.borrow();
                return Some((r.status_code, r.headers.clone(), r.content.clone_ref(py)));
            }
        }
        None
    });

    if let Some((status, headers, content)) = streaming_data {
        return Ok(build_streaming_response(content, status, headers));
    }

    // -- Normal / async handler -------------------------------------------
    let (r_status, r_body, mut r_headers) =
        execute_python_handler(exec_res, is_async, serializer, filter_response, schedule_coro, false, response_model.as_ref()).await;

    if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) {
        r_headers.insert("Content-Type".to_string(), "application/json".to_string());
    }

    // Dependency teardown generators.
    if !teardown_gens.is_empty() {
        tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| {
                if let Ok(builtins) = py.import_bound("builtins") {
                    for gen in teardown_gens {
                        let _ = builtins.call_method1("next", (&gen,));
                    }
                }
            });
        });
    }

    // Background tasks.
    if let Some(bg_obj) = bg_tasks_obj {
        let tasks: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> =
            Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
        if let Some(tasks) = tasks {
            let sc_bg  = schedule_coro.clone();
            let sem_bg = gil_sem.clone();
            tokio::spawn(async move {
                for (func, args, kw) in tasks {
                    let is_async = Python::with_gil(|py| {
                        py.import_bound("inspect").unwrap()
                            .getattr("iscoroutinefunction").unwrap()
                            .call1((func.bind(py),)).unwrap()
                            .extract::<bool>().unwrap_or(false)
                    });
                    if is_async {
                        let coro = Python::with_gil(|py| {
                            func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok()
                        });
                        if let Some(c) = coro {
                            let (tx, _rx) = oneshot::channel();
                            Python::with_gil(|py| {
                                if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                                    let _ = sc_bg.bind(py).call1((c, cb));
                                }
                            });
                        }
                    } else {
                        let sem = sem_bg.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            let _permit = sem.try_acquire().ok();
                            Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); });
                        }).await;
                    }
                }
            });
        }
    }

    let mut builder = HyperResponse::builder().status(r_status);
    for (k, v) in r_headers { builder = builder.header(&k, &v); }
    Ok(builder.body(Body::from(r_body)).unwrap())
}

// ---------------------------------------------------------------------------
// Decorator types
// ---------------------------------------------------------------------------

#[pyclass]
struct RouteDecorator {
    routes: Routes,
    method: String,
    path: String,
    is_ws: bool,
    response_model: Option<Py<PyAny>>,
    tags: Vec<String>,
    summary: Option<String>,
    description: Option<String>,
}

#[allow(non_local_definitions)]
#[pymethods]
impl RouteDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect  = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let sig      = inspect.call_method1("signature", (func.bind(py),))?;
        let params   = sig.getattr("parameters")?;

        let mut pydantic_model             = None;
        let mut pydantic_param_name        = None;
        let mut request_schema_json        = None;
        let mut request_param_name         = None;
        let mut background_task_param_name = None;
        let mut websocket_param_name       = None;
        let mut required_params            = Vec::new();
        let mut dependencies               = Vec::new();
        let mut param_names                = Vec::new();
        let mut param_types                = HashMap::new();

        let func_bound = func.bind(py);
        let func_globals = func_bound.getattr("__globals__").ok();

        let type_hints = if let Ok(typing) = py.import_bound("typing") {
            if let Some(ref g) = func_globals {
                typing.call_method1("get_type_hints", (func_bound, g)).ok()
            } else {
                typing.call_method1("get_type_hints", (func_bound,)).ok()
            }
        } else {
            None
        };

        if let Ok(values) = params.call_method0("values") {
            if let Ok(iter) = values.iter() {
                for p_res in iter {
                    let Ok(p) = p_res else { continue; };
                    let param_name: String = p.getattr("name")?.extract()?;
                    param_names.push(param_name.clone());

                    if param_name == "req" || param_name == "request" {
                        request_param_name = Some(param_name);
                        continue;
                    }
                    if self.is_ws {
                        websocket_param_name = Some(param_name.clone());
                        continue;
                    }

                    let default_val = p.getattr("default").ok();
                    let is_empty_default = default_val.as_ref()
                        .map(|d| d.to_string().contains("_empty"))
                        .unwrap_or(true);

                    let param_ann = if let Some(ref hints) = type_hints {
                        hints.get_item(&param_name).ok().or_else(|| p.getattr("annotation").ok())
                    } else {
                        p.getattr("annotation").ok()
                    };

                    if let Some(annotation) = param_ann {
                        let mut target_ann = annotation.clone();

                        if let Ok(str_name) = target_ann.extract::<String>() {
                            if let Some(ref g) = func_globals {
                                if let Ok(val) = g.get_item(&str_name) {
                                    target_ann = val;
                                }
                            }
                        }

                        if let Ok(args) = target_ann.getattr("__args__") {
                            if let Ok(first_arg) = args.get_item(0) {
                                target_ann = first_arg;
                            }
                        }

                        let has_mjs = target_ann.hasattr("model_json_schema").unwrap_or(false);
                        let has_sch = target_ann.hasattr("schema").unwrap_or(false);

                        if has_mjs {
                            pydantic_model      = Some(target_ann.clone().into());
                            pydantic_param_name = Some(param_name.clone());
                            if let Ok(schema_dict) = target_ann.call_method0("model_json_schema") {
                                if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) {
                                    if let Ok(s) = schema_str.extract::<String>() {
                                        request_schema_json = Some(s);
                                    }
                                }
                            }
                            continue;
                        } else if has_sch {
                            pydantic_model      = Some(target_ann.clone().into());
                            pydantic_param_name = Some(param_name.clone());
                            if let Ok(schema_dict) = target_ann.call_method0("schema") {
                                if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) {
                                    if let Ok(s) = schema_str.extract::<String>() {
                                        request_schema_json = Some(s);
                                    }
                                }
                            }
                            continue;
                        }

                        if let Ok(name) = target_ann.getattr("__name__") {
                            let type_name: String = name.extract().unwrap_or_default();
                            if type_name == "BackgroundTasks" {
                                background_task_param_name = Some(param_name.clone());
                                continue;
                            }
                            let pt = match type_name.as_str() {
                                "int"   => ParamType::Int,
                                "float" => ParamType::Float,
                                "bool"  => ParamType::Bool,
                                _       => ParamType::String,
                            };
                            param_types.insert(param_name.clone(), pt);
                        }
                    }

                    if let Some(ref d_val) = default_val {
                        let is_depends = d_val
                            .getattr("__class__")
                            .and_then(|cls| cls.getattr("__name__"))
                            .and_then(|n| n.extract::<String>())
                            .map(|n| n == "Depends")
                            .unwrap_or(false);
                        if is_depends {
                            let dep_func = if let Ok(explicit) = d_val.getattr("dependency") {
                                if explicit.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) }
                                else { explicit }
                            } else {
                                py.None().into_bound(py)
                            };
                            if !dep_func.is_none() {
                                let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                let is_dep_gen   = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                let dep_id       = dep_func.as_ptr() as isize;
                                dependencies.push(DependencyMeta {
                                    name: param_name.clone(), func: dep_func.into(),
                                    _is_async: is_dep_async, is_generator: is_dep_gen,
                                    use_cache: true, id: dep_id,
                                });
                            }
                            continue;
                        }
                    }

                    if is_empty_default && param_name != "req" && param_name != "request" && !self.is_ws {
                        required_params.push(param_name.clone());
                    }
                }
            }
        }

        self.routes.lock().unwrap().push(RouteEntry {
            method: self.method.clone(), original_path: self.path.clone(),
            segments: parse_pattern(&self.path), handler: func.clone_ref(py), is_async,
            pydantic_model, pydantic_param_name, request_schema_json,
            request_param_name, background_task_param_name, websocket_param_name,
            is_websocket: self.is_ws, dependencies, param_names, param_types, required_params,
            response_model: self.response_model.as_ref().map(|m| m.clone_ref(py)),
            tags: self.tags.clone(),
            summary: self.summary.clone(),
            description: self.description.clone(),
        });
        Ok(func)
    }
}

#[pyclass]
struct EventDecorator { handlers: Handlers }

#[pymethods]
impl EventDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let is_async = py.import_bound("inspect")?
            .getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        self.handlers.lock().unwrap().push((func.clone_ref(py), is_async));
        Ok(func.into_any())
    }
}

#[pyclass]
struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, description: Option<String> }

#[allow(non_local_definitions)]
#[pymethods]
impl ToolDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let doc: String = py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default();
        let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
        let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        self.tools.lock().unwrap().push(ToolEntry {
            name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()),
            _description: self.description.clone().unwrap_or(doc),
            schema_json: serde_json::from_str(&schema_str).unwrap(),
            handler: func.clone_ref(py), _is_async: is_async,
        });
        Ok(func)
    }
}

#[pyclass]
struct ResourceDecorator { resources: Resources, uri: String, mime_type: Option<String> }

#[allow(non_local_definitions)]
#[pymethods]
impl ResourceDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let doc: String = py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default();
        self.resources.lock().unwrap().push(ResourceEntry {
            uri: self.uri.clone(), description: doc,
            mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()),
            handler: func.clone_ref(py), _is_async: is_async,
        });
        Ok(func)
    }
}

#[pyclass]
struct PromptDecorator { prompts: Prompts, name: Option<String>, description: Option<String> }

#[allow(non_local_definitions)]
#[pymethods]
impl PromptDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        self.prompts.lock().unwrap().push(PromptEntry {
            name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()),
            description: self.description.clone().unwrap_or_default(),
            handler: func.clone_ref(py), _is_async: is_async,
        });
        Ok(func)
    }
}

#[pyclass(name = "Database")]
struct PyDatabase {
    sqlite_pool: Option<sqlx::SqlitePool>,
    pg_pool: Option<sqlx::PgPool>,
}

#[pymethods]
impl PyDatabase {
    #[pyo3(signature = (query))]
    fn execute(&self, py: Python<'_>, query: String) -> PyResult<u64> {
        let pool = self.sqlite_pool.clone();
        let pg = self.pg_pool.clone();
        let handle = tokio::runtime::Handle::try_current().ok();

        py.allow_threads(move || {
            let fut = async move {
                if let Some(sqlite) = pool {
                    let res = sqlx::query(&query).execute(&sqlite).await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok(res.rows_affected())
                } else if let Some(pg_p) = pg {
                    let res = sqlx::query(&query).execute(&pg_p).await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok(res.rows_affected())
                } else {
                    Err(pyo3::exceptions::PyRuntimeError::new_err("No active database pool"))
                }
            };
            if let Some(h) = handle {
                h.block_on(fut)
            } else {
                get_db_rt().block_on(fut)
            }
        })
    }

    #[pyo3(signature = (query))]
    fn query_json(&self, py: Python<'_>, query: String) -> PyResult<PyResponse> {
        use sqlx::{Column, Row, ValueRef};

        let pool = self.sqlite_pool.clone();
        let pg = self.pg_pool.clone();
        let handle = tokio::runtime::Handle::try_current().ok();

        let json_str: String = py.allow_threads(move || {
            let fut = async move {
                if let Some(sqlite) = pool {
                    let rows = sqlx::query(&query).fetch_all(&sqlite).await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    let mut json = String::with_capacity(rows.len() * 128);
                    json.push('[');
                    for (i, row) in rows.iter().enumerate() {
                        if i > 0 { json.push(','); }
                        json.push('{');
                        for (j, col) in row.columns().iter().enumerate() {
                            if j > 0 { json.push(','); }
                            let col_name = col.name();
                            json.push('"');
                            json.push_str(col_name);
                            json.push_str("\":");
                            if let Ok(i) = row.try_get::<i64, _>(col_name) {
                                json.push_str(&i.to_string());
                            } else if let Ok(s) = row.try_get::<String, _>(col_name) {
                                json.push('"');
                                json.push_str(&s.replace('\\', "\\\\").replace('"', "\\\""));
                                json.push('"');
                            } else if let Ok(f) = row.try_get::<f64, _>(col_name) {
                                json.push_str(&f.to_string());
                            } else if let Ok(b) = row.try_get::<bool, _>(col_name) {
                                json.push_str(if b { "true" } else { "false" });
                            } else {
                                json.push_str("null");
                            }
                        }
                        json.push('}');
                    }
                    json.push(']');
                    Ok::<_, PyErr>(json)
                } else if let Some(pg_p) = pg {
                    let rows = sqlx::query(&query).fetch_all(&pg_p).await
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    let mut records = Vec::new();
                    for row in rows {
                        let mut map = serde_json::Map::new();
                        for col in row.columns() {
                            let col_name = col.name();
                            let val: serde_json::Value = match row.try_get_raw(col_name) {
                                Ok(raw) if !raw.is_null() => {
                                    if let Ok(i) = row.try_get::<i64, _>(col_name) {
                                        serde_json::Value::Number(i.into())
                                    } else if let Ok(f) = row.try_get::<f64, _>(col_name) {
                                        serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)))
                                    } else if let Ok(b) = row.try_get::<bool, _>(col_name) {
                                        serde_json::Value::Bool(b)
                                    } else if let Ok(s) = row.try_get::<String, _>(col_name) {
                                        serde_json::Value::String(s)
                                    } else {
                                        serde_json::Value::Null
                                    }
                                }
                                _ => serde_json::Value::Null,
                            };
                            map.insert(col_name.to_string(), val);
                        }
                        records.push(serde_json::Value::Object(map));
                    }
                    Ok::<_, PyErr>(serde_json::to_string(&records).unwrap_or_else(|_| "[]".to_string()))
                } else {
                    Err(pyo3::exceptions::PyRuntimeError::new_err("No active database pool"))
                }
            };
            if let Some(h) = handle {
                h.block_on(fut)
            } else {
                get_db_rt().block_on(fut)
            }
        })?;

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        Ok(PyResponse {
            content: json_str.into_py(py),
            status_code: 200,
            headers,
        })
    }
}

// ---------------------------------------------------------------------------
// Phase C: Embedded Rust Power Modules
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (claims, secret, algorithm = None))]
fn encode_jwt(py: Python<'_>, claims: PyObject, secret: String, algorithm: Option<String>) -> PyResult<String> {
    let json_str: String = py.import_bound("json")?.call_method1("dumps", (claims,))?.extract()?;
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let alg = match algorithm.as_deref().unwrap_or("HS256") {
        "HS384" => jsonwebtoken::Algorithm::HS384,
        "HS512" => jsonwebtoken::Algorithm::HS512,
        _       => jsonwebtoken::Algorithm::HS256,
    };

    let header = jsonwebtoken::Header::new(alg);
    let key = jsonwebtoken::EncodingKey::from_secret(secret.as_bytes());

    jsonwebtoken::encode(&header, &val, &key)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JWT Encoding Error: {e}")))
}

#[pyfunction]
#[pyo3(signature = (token, secret, algorithm = None))]
fn decode_jwt(py: Python<'_>, token: String, secret: String, algorithm: Option<String>) -> PyResult<PyObject> {
    let alg = match algorithm.as_deref().unwrap_or("HS256") {
        "HS384" => jsonwebtoken::Algorithm::HS384,
        "HS512" => jsonwebtoken::Algorithm::HS512,
        _       => jsonwebtoken::Algorithm::HS256,
    };

    let mut validation = jsonwebtoken::Validation::new(alg);
    validation.required_spec_claims.clear();
    let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());

    let token_data = jsonwebtoken::decode::<serde_json::Value>(&token, &key, &validation)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JWT Decoding Error: {e}")))?;

    let json_bytes = serde_json::to_vec(&token_data.claims)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let py_json = py.import_bound("json")?;
    let py_dict = py_json.call_method1("loads", (PyBytes::new_bound(py, &json_bytes),))?;
    Ok(py_dict.unbind())
}

#[pyfunction]
#[pyo3(signature = (password))]
fn hash_password(py: Python<'_>, password: String) -> PyResult<String> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };

    py.allow_threads(move || {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    })
}

#[pyfunction]
#[pyo3(signature = (password, hash))]
fn verify_password(py: Python<'_>, password: String, hash: String) -> PyResult<bool> {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };

    py.allow_threads(move || {
        let parsed_hash = match PasswordHash::new(&hash) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };
        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
    })
}

#[pyfunction]
#[pyo3(signature = (template_str, context))]
fn render_template(py: Python<'_>, template_str: String, context: PyObject) -> PyResult<String> {
    let json_str: String = py.import_bound("json")?.call_method1("dumps", (context,))?.extract()?;
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let env = minijinja::Environment::new();
    env.render_str(&template_str, val)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Template Render Error: {e}")))
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

#[pymodule]
fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<PyRoute>()?;
    m.add_class::<PyRequest>()?;
    m.add_class::<PyResponse>()?;
    m.add_class::<PyUploadFile>()?;
    m.add_class::<PyWebSocket>()?;
    m.add_class::<EventDecorator>()?;
    m.add_class::<PyStreamingResponse>()?;
    m.add_class::<PyDatabase>()?;
    m.add_function(wrap_pyfunction!(encode_jwt, m)?)?;
    m.add_function(wrap_pyfunction!(decode_jwt, m)?)?;
    m.add_function(wrap_pyfunction!(hash_password, m)?)?;
    m.add_function(wrap_pyfunction!(verify_password, m)?)?;
    m.add_function(wrap_pyfunction!(render_template, m)?)?;
    Ok(())
}
