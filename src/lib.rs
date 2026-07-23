use pyo3::prelude::*;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server};
use tokio::sync::oneshot;

// ---------- Native File Watcher ----------

fn get_latest_mtime(dir: &Path) -> SystemTime {
    let mut max_time = SystemTime::UNIX_EPOCH;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name == "__pycache__" || name == "venv" || name == "env" {
                        continue;
                    }
                }
                let dir_time = get_latest_mtime(&path);
                if dir_time > max_time {
                    max_time = dir_time;
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("py") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > max_time {
                            max_time = mtime;
                        }
                    }
                }
            }
        }
    }
    max_time
}

// ---------- Route representation ----------

#[derive(Clone)]
enum Segment {
    Literal(String),
    Param(String),
}

struct RouteEntry {
    method: String,
    original_path: String,
    segments: Vec<Segment>,
    handler: Py<PyAny>,
    param_count: usize,
    is_async: bool,
    pydantic_model: Option<Py<PyAny>>,
    request_schema_json: Option<String>,
}

type Routes = Arc<Mutex<Vec<RouteEntry>>>;

// ---------- MCP registries ----------
// Same shape as RouteEntry/Routes above, but for MCP tools/resources/prompts.
// Kept separate from the HTTP route table since they're dispatched via
// JSON-RPC (POST /mcp) rather than path/method matching.

struct ToolEntry {
    name: String,
    description: String,
    schema: Option<Py<PyAny>>, // JSON-schema dict, built once at registration time
    handler: Py<PyAny>,
    is_async: bool,
}

struct ResourceEntry {
    uri: String,
    description: String,
    mime_type: String,
    handler: Py<PyAny>,
    is_async: bool,
}

struct PromptEntry {
    name: String,
    description: String,
    handler: Py<PyAny>,
    is_async: bool,
}

type Tools = Arc<Mutex<Vec<ToolEntry>>>;
type Resources = Arc<Mutex<Vec<ResourceEntry>>>;
type Prompts = Arc<Mutex<Vec<PromptEntry>>>;

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn parse_pattern(path: &str) -> Vec<Segment> {
    path_segments(path)
        .into_iter()
        .map(|s| {
            if s.starts_with('{') && s.ends_with('}') {
                Segment::Param(s[1..s.len() - 1].to_string())
            } else {
                Segment::Literal(s.to_string())
            }
        })
        .collect()
}

fn match_route(
    routes: &[RouteEntry],
    method: &str,
    path: &str,
) -> Option<(usize, HashMap<String, String>)> {
    let req_segs = path_segments(path);
    for (idx, r) in routes.iter().enumerate() {
        if r.method != method || r.segments.len() != req_segs.len() {
            continue;
        }
        let mut params = HashMap::new();
        let mut ok = true;
        for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
            match seg {
                Segment::Literal(l) => {
                    if l != val {
                        ok = false;
                        break;
                    }
                }
                Segment::Param(name) => {
                    params.insert(name.clone(), (*val).to_string());
                }
            }
        }
        if ok {
            return Some((idx, params));
        }
    }
    None
}

fn parse_query(query: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(q) = query {
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let mut it = pair.splitn(2, '=');
            let k = it.next().unwrap_or("");
            let v = it.next().unwrap_or("");
            let k = urlencoding::decode(k).unwrap_or_default().into_owned();
            let v = urlencoding::decode(v).unwrap_or_default().into_owned();
            map.insert(k, v);
        }
    }
    map
}

// ---------- OpenAPI & Swagger Generators ----------

fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    for seg in path.split('/') {
        if seg.starts_with('{') && seg.ends_with('}') {
            params.push(seg[1..seg.len() - 1].to_string());
        }
    }
    params
}

fn generate_openapi(routes: &[RouteEntry]) -> String {
    let mut paths_map: HashMap<String, Vec<&RouteEntry>> = HashMap::new();
    for r in routes {
        paths_map
            .entry(r.original_path.clone())
            .or_default()
            .push(r);
    }

    let mut paths_json = String::new();
    for (path, entries) in paths_map {
        let mut method_items = Vec::new();
        for r in entries {
            let method_lower = r.method.to_lowercase();
            let path_params = extract_path_params(&path);
            let mut parts = Vec::new();

            if !path_params.is_empty() {
                let mut p_arr = String::from(r#""parameters":["#);
                for (i, p) in path_params.iter().enumerate() {
                    if i > 0 { p_arr.push(','); }
                    p_arr.push_str(&format!(
                        r#"{{"name":"{}","in":"path","required":true,"schema":{{"type":"string"}}}}"#,
                        p
                    ));
                }
                p_arr.push(']');
                parts.push(p_arr);
            }

            if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
                let schema_json = r.request_schema_json.as_deref().unwrap_or(r#"{"type":"object","additionalProperties":true}"#);
                // Safe string concatenation to completely avoid format! brace conflicts
                let body_part = r#""requestBody":{"required":true,"content":{"application/json":{"schema":"#.to_string() 
                    + schema_json 
                    + "}}}";
                parts.push(body_part);
            }

            parts.push(r#""responses":{"200":{"description":"Successful Response"}}"#.to_string());

            let method_json = format!(r#""{}":{{{}}}"#, method_lower, parts.join(","));
            method_items.push(method_json);
        }
        paths_json.push_str(&format!(r#""{}":{{{}}},"#, path, method_items.join(",")));
    }
    if paths_json.ends_with(',') {
        paths_json.pop();
    }

    format!(
        r#"{{"openapi":"3.0.0","info":{{"title":"RustAPI","version":"0.1.0"}},"paths":{{{}}}}}"#,
        paths_json
    )
}

fn swagger_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Swagger UI - RustAPI</title>
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: '/openapi.json',
        dom_id: '#swagger-ui',
      });
    };
  </script>
</body>
</html>"#.to_string()
}

// ---------- Python-visible Request object ----------

#[pyclass]
struct PyRequest {
    #[pyo3(get)]
    method: String,
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    path_params: HashMap<String, String>,
    #[pyo3(get)]
    query_params: HashMap<String, String>,
    #[pyo3(get)]
    body: String,
}

#[pymethods]
impl PyRequest {
    #[new]
    fn new(
        method: String,
        path: String,
        path_params: HashMap<String, String>,
        query_params: HashMap<String, String>,
        body: String,
    ) -> Self {
        PyRequest { method, path, path_params, query_params, body }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = pyo3::types::PyDict::new_bound(py);
        dict.set_item("method", &self.method)?;
        dict.set_item("path", &self.path)?;
        
        let pp = pyo3::types::PyDict::new_bound(py);
        for (k, v) in &self.path_params { pp.set_item(k, v)?; }
        dict.set_item("path_params", pp)?;

        let qp = pyo3::types::PyDict::new_bound(py);
        for (k, v) in &self.query_params { qp.set_item(k, v)?; }
        dict.set_item("query_params", qp)?;

        dict.set_item("body", &self.body)?;
        Ok(dict.into())
    }

    fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json = py.import_bound("json")?;
        json.call_method1("loads", (&self.body,)).map(|v| v.into())
    }
}

// ---------- Engine ----------

#[pyclass]
struct Engine {
    routes: Routes,
    asgi_handler: PyObject,
    req_class: PyObject,
    // Pre-compiled Python callable that serializes a handler's return value to a
    // JSON string (falling back to `.model_dump()` for pydantic models). Built once
    // at Engine construction time instead of being eval()'d from a hand-built string
    // on every single request - faster, and immune to string-escaping mistakes.
    serializer: PyObject,
    // MCP (Model Context Protocol) registries. Same instance can serve both plain
    // HTTP routes and an MCP server over Streamable HTTP at POST /mcp.
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    schema_fn: PyObject,   // builds a JSON-schema dict from a function's type hints
    mcp_dispatch: PyObject, // async JSON-RPC dispatcher (initialize/tools/call/etc.)
}

#[allow(non_local_definitions)]
#[pymethods]
impl Engine {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let asgi_code = r#"
import asyncio
import inspect
import json
import typing
import urllib.parse

def _serialize_response(val):
    return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))

_JSON_TYPE_MAP = {str: "string", int: "integer", float: "number", bool: "boolean", list: "array", dict: "object"}

def _schema_from_signature(func):
    # Builds a JSON-schema "object" describing a function's parameters, from its
    # type hints. Used to auto-generate MCP tool/prompt input schemas so tools
    # don't need a hand-written pydantic model just to be exposed over MCP.
    sig = inspect.signature(func)
    properties = {}
    required = []
    for name, param in sig.parameters.items():
        ann = param.annotation
        py_type = ann if ann is not inspect.Parameter.empty else str
        optional = False
        if typing.get_origin(py_type) is typing.Union:
            args = [a for a in typing.get_args(py_type) if a is not type(None)]
            if len(args) == 1:
                py_type = args[0]
                optional = True
        json_type = _JSON_TYPE_MAP.get(py_type, "string")
        prop = {"type": json_type}
        if param.default is not inspect.Parameter.empty:
            prop["default"] = param.default if isinstance(param.default, (str, int, float, bool, type(None))) else None
        properties[name] = prop
        if param.default is inspect.Parameter.empty and not optional:
            required.append(name)
    schema = {"type": "object", "properties": properties}
    if required:
        schema["required"] = required
    return schema

MCP_PROTOCOL_VERSION = "2025-06-18"

async def handle_mcp_message(message, tools, resources, prompts):
    msg_id = message.get("id")
    method = message.get("method")
    params = message.get("params") or {}

    def ok(result):
        return {"jsonrpc": "2.0", "id": msg_id, "result": result}

    def err(code, msg):
        return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}

    if method == "initialize":
        return ok({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"},
        })

    if method in ("notifications/initialized", "initialized"):
        return None

    if method == "ping":
        return ok({})

    if method == "tools/list":
        items = [
            {"name": name, "description": meta.get("description") or "", "inputSchema": meta.get("schema") or {"type": "object", "properties": {}}}
            for name, meta in tools.items()
        ]
        return ok({"tools": items})

    if method == "tools/call":
        name = params.get("name")
        arguments = params.get("arguments") or {}
        meta = tools.get(name)
        if meta is None:
            return err(-32602, f"Unknown tool: {name}")
        handler = meta["handler"]
        try:
            if meta["is_async"]:
                result = await handler(**arguments)
            else:
                result = await asyncio.to_thread(handler, **arguments)
            text = result if isinstance(result, str) else _serialize_response(result)
            return ok({"content": [{"type": "text", "text": text}], "isError": False})
        except Exception as e:
            return ok({"content": [{"type": "text", "text": str(e)}], "isError": True})

    if method == "resources/list":
        items = [
            {"uri": uri, "name": meta.get("description") or uri, "mimeType": meta.get("mime_type") or "text/plain"}
            for uri, meta in resources.items()
        ]
        return ok({"resources": items})

    if method == "resources/read":
        uri = params.get("uri")
        meta = resources.get(uri)
        if meta is None:
            return err(-32602, f"Unknown resource: {uri}")
        handler = meta["handler"]
        result = await handler() if meta["is_async"] else await asyncio.to_thread(handler)
        text = result if isinstance(result, str) else _serialize_response(result)
        return ok({"contents": [{"uri": uri, "mimeType": meta.get("mime_type") or "text/plain", "text": text}]})

    if method == "prompts/list":
        items = [{"name": name, "description": meta.get("description") or "", "arguments": []} for name, meta in prompts.items()]
        return ok({"prompts": items})

    if method == "prompts/get":
        name = params.get("name")
        arguments = params.get("arguments") or {}
        meta = prompts.get(name)
        if meta is None:
            return err(-32602, f"Unknown prompt: {name}")
        handler = meta["handler"]
        text = await handler(**arguments) if meta["is_async"] else await asyncio.to_thread(handler, **arguments)
        return ok({"messages": [{"role": "user", "content": {"type": "text", "text": text}}]})

    if msg_id is None:
        return None  # unrecognized notification - do not respond

    return err(-32601, f"Method not found: {method}")

async def asgi_app(engine, req_class, scope, receive, send):
    if scope["type"] != "http":
        return

    method = scope["method"]
    path = scope.get("path", "/")

    if method == "GET" and path == "/docs":
        html = engine._swagger_html()
        await send({"type": "http.response.start", "status": 200, "headers": [[b"content-type", b"text/html"]]})
        await send({"type": "http.response.body", "body": html.encode("utf-8")})
        return
    elif method == "GET" and path == "/openapi.json":
        spec = engine._openapi_spec()
        await send({"type": "http.response.start", "status": 200, "headers": [[b"content-type", b"application/json"]]})
        await send({"type": "http.response.body", "body": spec.encode("utf-8")})
        return

    match = engine._dispatch(method, path)
    if match:
        handler, param_count, is_async, path_params, pydantic_model = match
        
        body_bytes = b""
        more_body = True
        while more_body:
            message = await receive()
            body_bytes += message.get("body", b"")
            more_body = message.get("more_body", False)
            
        body_str = body_bytes.decode("utf-8", errors="replace")
        
        query_string = scope.get("query_string", b"").decode("utf-8")
        query_params = {}
        if query_string:
            for pair in query_string.split("&"):
                if "=" in pair:
                    k, v = pair.split("=", 1)
                    query_params[urllib.parse.unquote_plus(k)] = urllib.parse.unquote_plus(v)
        
        args = ()
        if param_count > 0:
            if pydantic_model is not None:
                try:
                    body_data = json.loads(body_str) if body_str else {}
                except Exception:
                    body_data = {}
                arg = pydantic_model.model_validate(body_data)
                args = (arg,)
            else:
                req = req_class(method, path, path_params, query_params, body_str)
                args = (req,)
            
        try:
            if is_async:
                res = await handler(*args)
            else:
                res = await asyncio.to_thread(handler, *args)
            
            body_res = _serialize_response(res).encode("utf-8")
            status = 200
        except Exception as e:
            res = {"error": f"Internal Server Error: {str(e)}"}
            body_res = json.dumps(res).encode("utf-8")
            status = 500
    else:
        res = {"error": "Not Found"}
        body_res = json.dumps(res).encode("utf-8")
        status = 404
        
    await send({"type": "http.response.start", "status": status, "headers": [[b"content-type", b"application/json"]]})
    await send({"type": "http.response.body", "body": body_res})
"#;
        let module = PyModule::from_code_bound(py, asgi_code, "asgi_internal.py", "asgi_internal")?;
        let asgi_handler = module.getattr("asgi_app")?.into();
        let req_class = py.get_type_bound::<PyRequest>().into();
        let serializer = module.getattr("_serialize_response")?.into();
        let schema_fn = module.getattr("_schema_from_signature")?.into();
        let mcp_dispatch = module.getattr("handle_mcp_message")?.into();

        Ok(Engine {
            routes: Arc::new(Mutex::new(Vec::new())),
            asgi_handler,
            req_class,
            serializer,
            tools: Arc::new(Mutex::new(Vec::new())),
            resources: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
            schema_fn,
            mcp_dispatch,
        })
    }

    fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path } }
    fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path } }
    fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path } }
    fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path } }

    // ---- MCP: same Engine instance can expose tools/resources/prompts over
    // JSON-RPC at POST /mcp, alongside its normal HTTP routes. ----

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

    #[pyo3(signature = (scope, receive, send))]
    fn __call__<'py>(
        slf: &Bound<'py, Self>, 
        py: Python<'py>, 
        scope: PyObject, 
        receive: PyObject, 
        send: PyObject
    ) -> PyResult<Bound<'py, PyAny>> {
        let engine = slf.borrow();
        engine.asgi_handler.bind(py).call1((slf.clone(), engine.req_class.clone_ref(py), scope, receive, send))
    }

    fn _dispatch(&self, py: Python<'_>, method: String, path: String) -> PyResult<Option<(PyObject, usize, bool, HashMap<String, String>, Option<PyObject>)>> {
        let guard = self.routes.lock().unwrap();
        match match_route(&guard, &method, &path) {
            Some((idx, path_params)) => {
                let entry = &guard[idx];
                let model_obj = entry.pydantic_model.as_ref().map(|m| m.clone_ref(py));
                Ok(Some((entry.handler.clone_ref(py), entry.param_count, entry.is_async, path_params, model_obj)))
            }
            None => Ok(None),
        }
    }

    fn _swagger_html(&self) -> String { swagger_html() }
    fn _openapi_spec(&self) -> String {
        let guard = self.routes.lock().unwrap();
        generate_openapi(&guard)
    }

    #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false))]
    fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool) -> PyResult<()> {
        let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();

        if reload && !is_worker {
            println!("👀 Auto-reload is enabled. Watching for .py file changes...");

            let sys = py.import_bound("sys")?;
            let executable: String = sys.getattr("executable")?.extract()?;
            let argv: Vec<String> = sys.getattr("argv")?.extract()?;

            let exit_result: Result<(), PyErr> = py.allow_threads(move || {
                let mut child = Command::new(&executable)
                    .args(&argv)
                    .env("RUSTAPI_WORKER", "1")
                    .spawn()
                    .expect("Failed to start worker process");

                let root_dir = Path::new(".");
                let mut last_mtime = get_latest_mtime(root_dir);

                loop {
                    let current_mtime = get_latest_mtime(root_dir);
                    if current_mtime > last_mtime {
                        println!("🔄 File change detected! Restarting server...\n");
                        let _ = child.kill();
                        let _ = child.wait();

                        child = Command::new(&executable)
                            .args(&argv)
                            .env("RUSTAPI_WORKER", "1")
                            .spawn()
                            .expect("Failed to restart worker process");

                        last_mtime = current_mtime;
                    }

                    if let Err(e) = Python::with_gil(|py| py.check_signals()) {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(e);
                    }
                    thread::sleep(Duration::from_millis(250));
                }
            });

            if let Err(err) = exit_result {
                return Python::with_gil(|py| {
                    if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
                        println!("\n[INFO] Server shut down successfully.");
                        Ok(())
                    } else {
                        Err(err)
                    }
                });
            }
            return Ok(());
        }

        let routes = self.routes.clone();
        let serializer = self.serializer.clone_ref(py);
        let tools = self.tools.clone();
        let resources = self.resources.clone();
        let prompts = self.prompts.clone();
        let mcp_dispatch = self.mcp_dispatch.clone_ref(py);
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e: std::net::AddrParseError| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        if !is_worker {
            println!("🚀 rustapi listening on http://{addr}");
        } else {
            println!("🚀 Worker started. Listening on http://{addr}");
        }
        println!("📄 Swagger UI docs available at http://{addr}/docs");
        println!("Press Ctrl+C to stop the server.");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<()>();

        let server_handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
            rt.block_on(async move {
                let make_svc = make_service_fn(move |_conn| {
                    let routes = routes.clone();
                    let serializer = serializer.clone();
                    let tools = tools.clone();
                    let resources = resources.clone();
                    let prompts = prompts.clone();
                    let mcp_dispatch = mcp_dispatch.clone();
                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| handle(
                            req,
                            routes.clone(),
                            serializer.clone(),
                            tools.clone(),
                            resources.clone(),
                            prompts.clone(),
                            mcp_dispatch.clone(),
                        )))
                    }
                });

                let server = Server::bind(&addr).serve(make_svc);
                let graceful = server.with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });

                if let Err(e) = graceful.await {
                    eprintln!("Server error: {e}");
                }
            });
            let _ = done_tx.send(());
        });

        let pending_err = py.allow_threads(move || {
            loop {
                if let Ok(()) = done_rx.try_recv() {
                    return None;
                }
                if let Err(err) = Python::with_gil(|py| py.check_signals()) {
                    let _ = shutdown_tx.send(());
                    return Some(err);
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        let _ = server_handle.join();
        if let Some(err) = pending_err {
            Python::with_gil(|py| {
                if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
                    if !is_worker {
                        println!("\n[INFO] Server shut down successfully.");
                    }
                    Ok(())
                } else {
                    Err(err)
                }
            })
        } else {
            Ok(())
        }
    }
}

async fn handle(
    req: HyperRequest<Body>,
    routes: Routes,
    serializer: PyObject,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    mcp_dispatch: PyObject,
) -> Result<HyperResponse<Body>, Infallible> {
    let start_time = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_params = parse_query(req.uri().query());

    let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    let (status, resp_body, content_type) = if method == "GET" && path == "/docs" {
        (200, swagger_html(), "text/html")
    } else if method == "GET" && path == "/openapi.json" {
        let spec = {
            let guard = routes.lock().unwrap();
            generate_openapi(&guard)
        };
        (200, spec, "application/json")
    } else if method == "POST" && path == "/mcp" {
        // MCP Streamable HTTP transport (simplified): one JSON-RPC message in,
        // one JSON response out. No SSE/server-push in this version.
        let outcome = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<(u16, String), PyErr> {
                let json_mod = py.import_bound("json")?;
                let message = json_mod.call_method1("loads", (&body,))?;

                let tools_dict = pyo3::types::PyDict::new_bound(py);
                for t in tools.lock().unwrap().iter() {
                    let meta = pyo3::types::PyDict::new_bound(py);
                    meta.set_item("handler", t.handler.clone_ref(py))?;
                    meta.set_item("is_async", t.is_async)?;
                    meta.set_item("schema", t.schema.as_ref().map(|s| s.clone_ref(py)))?;
                    meta.set_item("description", &t.description)?;
                    tools_dict.set_item(&t.name, meta)?;
                }

                let resources_dict = pyo3::types::PyDict::new_bound(py);
                for r in resources.lock().unwrap().iter() {
                    let meta = pyo3::types::PyDict::new_bound(py);
                    meta.set_item("handler", r.handler.clone_ref(py))?;
                    meta.set_item("is_async", r.is_async)?;
                    meta.set_item("description", &r.description)?;
                    meta.set_item("mime_type", &r.mime_type)?;
                    resources_dict.set_item(&r.uri, meta)?;
                }

                let prompts_dict = pyo3::types::PyDict::new_bound(py);
                for p in prompts.lock().unwrap().iter() {
                    let meta = pyo3::types::PyDict::new_bound(py);
                    meta.set_item("handler", p.handler.clone_ref(py))?;
                    meta.set_item("is_async", p.is_async)?;
                    meta.set_item("description", &p.description)?;
                    prompts_dict.set_item(&p.name, meta)?;
                }

                let coro = mcp_dispatch.bind(py).call1((message, tools_dict, resources_dict, prompts_dict))?;
                let asyncio = py.import_bound("asyncio")?;
                let result = asyncio.call_method1("run", (coro,))?;

                if result.is_none() {
                    // JSON-RPC notification - no response body per spec.
                    Ok((202, String::new()))
                } else {
                    let serialized: String = serializer.bind(py).call1((result,))?.extract()?;
                    Ok((200, serialized))
                }
            })
        }).await;

        match outcome {
            Ok(Ok((s, b))) => (s, b, "application/json"),
            Ok(Err(e)) => (500, format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'")), "application/json"),
            Err(_) => (500, r#"{"error":"Rust background task panicked"}"#.to_string(), "application/json"),
        }
    } else {
        let matched = {
            let guard = routes.lock().unwrap();
            match_route(&guard, &method, &path)
        };

        match matched {
            Some((idx, path_params)) => {
                let (handler, param_count, is_async, pydantic_model) = {
                    let guard = routes.lock().unwrap();
                    let entry = &guard[idx];
                    Python::with_gil(|py| (
                        entry.handler.clone_ref(py), 
                        entry.param_count, 
                        entry.is_async, 
                        entry.pydantic_model.as_ref().map(|m| m.clone_ref(py))
                    ))
                };

                let method2 = method.clone();
                let path2 = path.clone();

                let outcome = tokio::task::spawn_blocking(move || {
                    Python::with_gil(|py| -> Result<(u16, String), PyErr> {
                        let result = if param_count == 0 {
                            handler.call0(py)?
                        } else if let Some(ref model) = pydantic_model {
                            let json_mod = py.import_bound("json")?;
                            let py_dict = if body.is_empty() {
                                pyo3::types::PyDict::new_bound(py).into_any()
                            } else {
                                json_mod.call_method1("loads", (&body,))?.into_any()
                            };
                            let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
                            handler.call1(py, (instance,))?
                        } else {
                            let req_obj = Py::new(py, PyRequest { 
                                method: method2, path: path2, path_params, query_params, body 
                            })?;
                            handler.call1(py, (req_obj,))?
                        };

                        let val = if is_async {
                            let asyncio = py.import_bound("asyncio")?;
                            asyncio.call_method1("run", (result,))?
                        } else {
                            result.into_bound(py)
                        };

                        // Call the pre-compiled Python serializer function instead of
                        // eval()-ing a hand-built code string on every request.
                        let serialized: String = serializer.bind(py).call1((val,))?.extract()?;

                        Ok((200, serialized))
                    })
                }).await;

                match outcome {
                    Ok(Ok((s, b))) => (s, b, "application/json"),
                    Ok(Err(e)) => (500, format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'")), "application/json"),
                    Err(_) => (500, r#"{"error":"Rust background task panicked"}"#.to_string(), "application/json"),
                }
            }
            None => (404, r#"{"error":"not found"}"#.to_string(), "application/json"),
        }
    };

    let duration = start_time.elapsed().as_millis();
    println!("[INFO] {} {} - {} ({}ms)", method, path, status, duration);

    Ok(HyperResponse::builder()
        .status(status)
        .header("Content-Type", content_type)
        .body(Body::from(resp_body))
        .unwrap())
}

#[pyclass]
struct RouteDecorator {
    routes: Routes,
    method: String,
    path: String,
}

#[allow(non_local_definitions)]
#[pymethods]
impl RouteDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        
        let iscoroutinefunction = inspect.getattr("iscoroutinefunction")?;
        let is_async: bool = iscoroutinefunction.call1((func.bind(py),))?.extract()?;

        let sig = inspect.call_method1("signature", (func.bind(py),))?;
        let params = sig.getattr("parameters")?;
        let param_count: usize = params.call_method0("__len__")?.extract()?;

        let mut pydantic_model: Option<Py<PyAny>> = None;
        let mut request_schema_json: Option<String> = None;

        if let Ok(params_dict) = params.call_method0("values") {
            if let Ok(iter) = params_dict.iter() {
                for p_res in iter {
                    if let Ok(p) = p_res {
                        if let Ok(annotation) = p.getattr("annotation") {
                            if annotation.hasattr("model_json_schema").unwrap_or(false) {
                                pydantic_model = Some(annotation.clone().into());
                                if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
                                    if let Ok(json_mod) = py.import_bound("json") {
                                        if let Ok(schema_str) = json_mod.call_method1("dumps", (schema_dict,)) {
                                            if let Ok(s) = schema_str.extract::<String>() {
                                                request_schema_json = Some(s);
                                            }
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        self.routes.lock().unwrap().push(RouteEntry {
            method: self.method.clone(),
            original_path: self.path.clone(),
            segments: parse_pattern(&self.path),
            handler: func.clone_ref(py),
            param_count,
            is_async,
            pydantic_model,
            request_schema_json,
        });
        Ok(func)
    }
}

// ---------- MCP decorators ----------
// Mirror RouteDecorator above: Engine.tool()/.resource()/.prompt() return one of
// these, which registers the wrapped function on __call__ and hands it straight
// back unmodified (so it stays a normal, directly-callable Python function).

#[pyclass]
struct ToolDecorator {
    tools: Tools,
    schema_fn: PyObject,
    name: Option<String>,
    description: Option<String>,
}

#[allow(non_local_definitions)]
#[pymethods]
impl ToolDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect
            .getattr("iscoroutinefunction")?
            .call1((func.bind(py),))?
            .extract()?;
        let fname: String = func.bind(py).getattr("__name__")?.extract()?;
        let doc: Option<String> = inspect
            .call_method1("getdoc", (func.bind(py),))?
            .extract()
            .unwrap_or(None);

        let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;

        self.tools.lock().unwrap().push(ToolEntry {
            name: self.name.clone().unwrap_or(fname),
            description: self.description.clone().or(doc).unwrap_or_default(),
            schema: Some(schema_obj.into()),
            handler: func.clone_ref(py),
            is_async,
        });
        Ok(func)
    }
}

#[pyclass]
struct ResourceDecorator {
    resources: Resources,
    uri: String,
    mime_type: Option<String>,
}

#[allow(non_local_definitions)]
#[pymethods]
impl ResourceDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect
            .getattr("iscoroutinefunction")?
            .call1((func.bind(py),))?
            .extract()?;
        let doc: Option<String> = inspect
            .call_method1("getdoc", (func.bind(py),))?
            .extract()
            .unwrap_or(None);

        self.resources.lock().unwrap().push(ResourceEntry {
            uri: self.uri.clone(),
            description: doc.unwrap_or_default(),
            mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()),
            handler: func.clone_ref(py),
            is_async,
        });
        Ok(func)
    }
}

#[pyclass]
struct PromptDecorator {
    prompts: Prompts,
    name: Option<String>,
    description: Option<String>,
}

#[allow(non_local_definitions)]
#[pymethods]
impl PromptDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect
            .getattr("iscoroutinefunction")?
            .call1((func.bind(py),))?
            .extract()?;
        let fname: String = func.bind(py).getattr("__name__")?.extract()?;
        let doc: Option<String> = inspect
            .call_method1("getdoc", (func.bind(py),))?
            .extract()
            .unwrap_or(None);

        self.prompts.lock().unwrap().push(PromptEntry {
            name: self.name.clone().unwrap_or(fname),
            description: self.description.clone().or(doc).unwrap_or_default(),
            handler: func.clone_ref(py),
            is_async,
        });
        Ok(func)
    }
}

#[pymodule]
fn rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<PyRequest>()?;
    Ok(())
}