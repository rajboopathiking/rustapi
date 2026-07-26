use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3::types::{PyDict, PyString};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server};
use notify::{RecursiveMode, Watcher};
use serde_json::json;
use tokio::sync::{oneshot, Semaphore};

const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

#[derive(Clone)]
enum Segment {
    Literal(String),
    Param(String),
}

struct DependencyMeta {
    name: String,
    func: Py<PyAny>,
    is_async: bool,
    is_generator: bool,
    use_cache: bool,
    id: isize,
}

impl Clone for DependencyMeta {
    fn clone(&self) -> Self {
        Python::with_gil(|py| DependencyMeta {
            name: self.name.clone(),
            func: self.func.clone_ref(py),
            is_async: self.is_async,
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
    dependencies: Vec<DependencyMeta>,
}

type Routes = Arc<Mutex<Vec<RouteEntry>>>;

struct ToolEntry {
    name: String,
    description: String,
    schema_json: serde_json::Value,
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
                    if l != val { ok = false; break; }
                }
                Segment::Param(name) => {
                    params.insert(name.clone(), (*val).to_string());
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
    let mut paths = serde_json::Map::new();

    for r in routes {
        let path_params = extract_path_params(&r.original_path);
        let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });

        if !path_params.is_empty() {
            let mut params = Vec::new();
            for p in path_params {
                params.push(json!({ "name": p, "in": "path", "required": true, "schema": { "type": "string" } }));
            }
            method_obj["parameters"] = json!(params);
        }

        if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
            let schema: serde_json::Value = r.request_schema_json.as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| json!({"type":"object","additionalProperties":true}));
            
            method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": schema } } });
        }

        let method_lower = r.method.to_lowercase();
        if let Some(path_item) = paths.get_mut(&r.original_path) {
            path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
        } else {
            paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
        }
    }

    serde_json::to_string(&json!({ "openapi": "3.0.0", "info": { "title": "RustAPI", "version": "0.1.0" }, "paths": paths })).unwrap()
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
    window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };
</script>
</body>
</html>"#.to_string()
}

#[pyclass]
struct PyRequest {
    #[pyo3(get)] method: String,
    #[pyo3(get)] path: String,
    #[pyo3(get)] path_params: HashMap<String, String>,
    #[pyo3(get)] query_params: HashMap<String, String>,
    #[pyo3(get)] headers: HashMap<String, String>,
    #[pyo3(get)] cookies: HashMap<String, String>,
    #[pyo3(get)] body: String,
}

#[pymethods]
impl PyRequest {
    #[new]
    fn new(method: String, path: String, path_params: HashMap<String, String>, query_params: HashMap<String, String>, headers: HashMap<String, String>, cookies: HashMap<String, String>, body: String) -> Self {
        PyRequest { method, path, path_params, query_params, headers, cookies, body }
    }

    fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        py.import_bound("json")?.call_method1("loads", (&self.body,)).map(|v| v.into())
    }
}

#[pyclass(name = "Response")]
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
        PyResponse {
            content,
            status_code,
            headers: headers.unwrap_or_default(),
        }
    }
}

#[pyclass]
struct CoroCallback {
    tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>>,
}

#[pymethods]
impl CoroCallback {
    #[pyo3(signature = (result, error))]
    fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            if error.is_none(py) {
                let _ = tx.send(Ok(result));
            } else {
                let _ = tx.send(Err(error));
            }
        }
    }
}

#[pyclass]
struct Engine {
    routes: Routes,
    serializer: PyObject,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    schema_fn: PyObject,
    schedule_coro_fn: PyObject,
}

#[allow(non_local_definitions)]
#[pymethods]
impl Engine {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let python_code = r#"
import asyncio
import inspect
import json
import typing
import threading

_engine_loop = asyncio.new_event_loop()

def _start_engine_loop():
    asyncio.set_event_loop(_engine_loop)
    _engine_loop.run_forever()

threading.Thread(target=_start_engine_loop, daemon=True).start()

def _schedule_coro(coro, callback):
    def done_cb(fut):
        try:
            res = fut.result()
            callback(res, None)
        except Exception as e:
            callback(None, e)
    fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
    fut.add_done_callback(done_cb)

def _serialize_response(val):
    return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))

_JSON_TYPE_MAP = {str: "string", int: "integer", float: "number", bool: "boolean", list: "array", dict: "object"}

def _schema_from_signature(func):
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
"#;
        let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
        
        Ok(Engine {
            routes: Arc::new(Mutex::new(Vec::new())),
            serializer: module.getattr("_serialize_response")?.into(),
            schedule_coro_fn: module.getattr("_schedule_coro")?.into(),
            schema_fn: module.getattr("_schema_from_signature")?.into(),
            tools: Arc::new(Mutex::new(Vec::new())),
            resources: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path } }
    fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path } }
    fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path } }
    fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path } }
    fn options(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "OPTIONS".into(), path } }
    fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path } }
    fn head(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "HEAD".into(), path } }

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

    #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
    fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
        let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
        let safe_workers = if workers < 1 { 1 } else { workers };

        if (reload || safe_workers > 1) && !is_worker {
            println!("🚀 Starting Master process (PID {}) spanning {} worker(s)...", std::process::id(), safe_workers);
            if reload {
                println!("👀 Auto-reload enabled. Watching for .py file changes...");
            }

            let sys = py.import_bound("sys")?;
            let executable: String = sys.getattr("executable")?.extract()?;
            let argv: Vec<String> = sys.getattr("argv")?.extract()?;

            let exit_result: Result<(), PyErr> = py.allow_threads(move || {
                let spawn_children = || {
                    let mut new_children = Vec::new();
                    for i in 0..safe_workers {
                        let child = Command::new(&executable)
                            .args(&argv)
                            .env("RUSTAPI_WORKER", i.to_string())
                            .spawn()
                            .expect("Failed to start worker process");
                        new_children.push(child);
                    }
                    new_children
                };

                let mut children = spawn_children();

                let (tx, rx) = std::sync::mpsc::channel();
                let _watcher_keepalive = if reload {
                    let mut watcher = notify::recommended_watcher(tx).unwrap();
                    watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap();
                    Some(watcher)
                } else {
                    None
                };

                loop {
                    if reload {
                        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
                            if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
                                println!("🔄 File change detected! Restarting all {} workers...\n", safe_workers);
                                for mut child in children {
                                    let _ = child.kill();
                                    let _ = child.wait();
                                }
                                children = spawn_children();
                                continue;
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(250));
                    }

                    if let Err(e) = Python::with_gil(|py| py.check_signals()) {
                        for mut child in children {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                        return Err(e);
                    }
                }
            });

            if let Err(err) = exit_result {
                return Python::with_gil(|py| {
                    if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
                        println!("\n[INFO] Master process shut down successfully.");
                        Ok(())
                    } else { Err(err) }
                });
            }
            return Ok(());
        }

        let worker_id = std::env::var("RUSTAPI_WORKER").unwrap_or_else(|_| "0".to_string());
        
        let routes = self.routes.clone();
        let tools = self.tools.clone();
        let resources = self.resources.clone();
        let prompts = self.prompts.clone();
        let serializer_arc = Arc::new(self.serializer.clone_ref(py));
        let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
        
        let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
        
        let domain = if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 };
        let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None).unwrap();
        socket.set_reuse_address(true).unwrap();
        #[cfg(unix)]
        socket.set_reuse_port(true).unwrap();
        
        socket.bind(&addr.into()).unwrap();
        socket.listen(1024).unwrap();
        
        let std_listener: std::net::TcpListener = socket.into();
        std_listener.set_nonblocking(true).unwrap();

        if worker_id == "0" {
            println!("🚀 rustapi listening on http://{addr}");
        }

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<()>();

        let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));

        let server_handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .max_blocking_threads(num_cpus * 4)
                .build()
                .expect("failed to build tokio runtime");

            rt.block_on(async move {
                let make_svc = make_service_fn(move |_conn| {
                    let routes = routes.clone();
                    let tools = tools.clone();
                    let resources = resources.clone();
                    let prompts = prompts.clone();
                    let serializer = serializer_arc.clone();
                    let schedule_coro = schedule_coro_arc.clone();
                    let sem = gil_semaphore.clone();

                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| handle(
                            req, routes.clone(), serializer.clone(), schedule_coro.clone(),
                            tools.clone(), resources.clone(), prompts.clone(), sem.clone(),
                        )))
                    }
                });

                let server = Server::from_tcp(std_listener).unwrap()
                    .http1_keepalive(true)
                    .tcp_nodelay(true)
                    .serve(make_svc);
                    
                let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
                if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
            });
            let _ = done_tx.send(());
        });

        let pending_err = py.allow_threads(move || {
            loop {
                if let Ok(()) = done_rx.try_recv() { return None; }
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
                if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) }
            })
        } else { Ok(()) }
    }
}

async fn handle(
    req: HyperRequest<Body>,
    routes: Routes,
    serializer: Arc<PyObject>,
    schedule_coro: Arc<PyObject>,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    gil_sem: Arc<Semaphore>,
) -> Result<HyperResponse<Body>, Infallible> {
    let start_time = Instant::now();
    
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_params = parse_query(req.uri().query());

    let mut headers_map = HashMap::new();
    let mut cookies_map = HashMap::new();
    
    for (k, v) in req.headers() {
        let key_str = k.as_str().to_string();
        let val_str = v.to_str().unwrap_or("").to_string();
        
        if key_str.eq_ignore_ascii_case("cookie") {
            for pair in val_str.split(';') {
                let mut parts = pair.trim().splitn(2, '=');
                if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) {
                    cookies_map.insert(ck.to_string(), cv.to_string());
                }
            }
        }
        headers_map.insert(key_str, val_str);
    }

    let mut body_bytes = Vec::new();
    let mut body_stream = req.into_body();
    while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
        let chunk = chunk_res.unwrap_or_default();
        if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE {
            return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap());
        }
        body_bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string());
        (200, swagger_html(), h)
    } else if method == "GET" && path == "/openapi.json" {
        let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
        (200, spec, h)
    } else if method == "POST" && path == "/mcp" {
        let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
        let req_method = req_json["method"].as_str().unwrap_or("").to_string();
        let has_id = req_json.get("id").is_some();
        let msg_id = req_json["id"].clone();
        let params = req_json.get("params").unwrap_or(&json!({})).clone();

        let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
        let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

        let result = if !has_id {
            String::new()
        } else if req_method == "initialize" {
            ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}}))
        } else if req_method == "notifications/initialized" || req_method == "initialized" {
            String::new()
        } else if req_method == "ping" {
            ok(json!({}))
        } else if req_method == "tools/list" {
            let guard = tools.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })).collect();
            ok(json!({"tools": items}))
        } else if req_method == "resources/list" {
            let guard = resources.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
            ok(json!({"resources": items}))
        } else if req_method == "prompts/list" {
            let guard = prompts.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
            ok(json!({"prompts": items}))
        } else if req_method == "tools/call" {
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args_json = params["arguments"].clone();
            let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async)));

            if let Some((handler, is_async)) = tool_opt {
                let _permit = gil_sem.acquire().await.ok();
                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                    Python::with_gil(|py| -> PyResult<PyObject> {
                        let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                        if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
                    })
                }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));
                
                let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
                if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
            } else { err(-32602, &format!("Unknown tool: {}", name)) }
        } else if req_method == "resources/read" {
            let uri = params["uri"].as_str().unwrap_or("").to_string();
            let res_opt = Python::with_gil(|py| resources.lock().unwrap().iter().find(|r| r.uri == uri).map(|r| (r.handler.clone_ref(py), r.is_async, r.mime_type.clone())));
            if let Some((handler, is_async, mime)) = res_opt {
                let _permit = gil_sem.acquire().await.ok();
                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || Python::with_gil(|py| handler.call0(py))).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));
                let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
                if t_status < 400 { ok(json!({"contents": [{"uri": uri, "mimeType": mime, "text": content}]})) } else { err(-32603, &content) }
            } else { err(-32602, &format!("Unknown resource: {}", uri)) }
        } else if req_method == "prompts/get" {
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args_json = params["arguments"].clone();
            let pro_opt = Python::with_gil(|py| prompts.lock().unwrap().iter().find(|p| p.name == name).map(|p| (p.handler.clone_ref(py), p.is_async)));
            if let Some((handler, is_async)) = pro_opt {
                let _permit = gil_sem.acquire().await.ok();
                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                    Python::with_gil(|py| -> PyResult<PyObject> {
                        let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                        if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
                    })
                }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));
                let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
                if t_status < 400 { ok(json!({"messages": [{"role": "user", "content": {"type": "text", "text": content}}]})) } else { err(-32603, &content) }
            } else { err(-32602, &format!("Unknown prompt: {}", name)) }
        } else { err(-32601, &format!("Method not found: {}", req_method)) };

        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
        if result.is_empty() { (202, result, h) } else { (200, result, h) }
    } else {
        let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };

        match matched {
            Some((idx, path_params)) => {
                let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, deps) = Python::with_gil(|py| {
                    let guard = routes.lock().unwrap();
                    let entry = &guard[idx];
                    (
                        entry.handler.clone_ref(py),
                        entry.is_async,
                        entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)),
                        entry.pydantic_param_name.clone(),
                        entry.request_param_name.clone(),
                        entry.dependencies.clone()
                    )
                });

                let method_c = method.clone();
                let path_c = path.clone();
                let body_c = body.clone();
                let headers_c = headers_map.clone();
                let cookies_c = cookies_map.clone();
                let path_params_c = path_params.clone();
                let mut dependency_error: Option<String> = None;

                let mut resolved_args: HashMap<String, PyObject> = HashMap::new();
                let mut cache: HashMap<isize, PyObject> = HashMap::new();
                let mut teardown_generators: Vec<PyObject> = Vec::new();

                for dep in deps {
                    if dep.use_cache && cache.contains_key(&dep.id) {
                        let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
                        resolved_args.insert(dep.name.clone(), cached_val);
                        continue;
                    }

                    let dep_result_res: Result<PyObject, String> = if dep.is_async {
                        let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map(|obj| obj.into()).map_err(|e| e.to_string()));
                        match coro_res {
                            Ok(coro) => {
                                let (tx, rx) = oneshot::channel();
                                Python::with_gil(|py| {
                                    if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) {
                                        let _ = schedule_coro.bind(py).call1((coro, cb));
                                    }
                                });
                                match rx.await {
                                    Ok(Ok(res)) => Ok(res),
                                    Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())),
                                    Err(_) => Err("Asyncio channel dropped".to_string()),
                                }
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        let sem_clone = gil_sem.clone();
                        let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
                        tokio::task::spawn_blocking(move || {
                            let _permit = sem_clone.try_acquire().ok();
                            Python::with_gil(|py| dep_func.call0(py).map(|obj| obj.into()).map_err(|e| e.to_string()))
                        }).await.unwrap_or_else(|_| Err("Worker thread panicked".to_string()))
                    };

                    match dep_result_res {
                        Ok(dep_obj) => {
                            let val_res: Result<PyObject, String> = Python::with_gil(|py| {
                                if dep.is_generator {
                                    let builtins = py.import_bound("builtins").map_err(|e| e.to_string())?;
                                    let val = builtins.call_method1("next", (&dep_obj,)).map_err(|e| e.to_string())?.into();
                                    Ok(val)
                                } else {
                                    Ok(dep_obj.clone_ref(py))
                                }
                            });

                            match val_res {
                                Ok(val) => {
                                    if dep.is_generator { teardown_generators.push(dep_obj); }
                                    if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); }
                                    resolved_args.insert(dep.name, val);
                                }
                                Err(e) => { dependency_error = Some(e); break; }
                            }
                        }
                        Err(e) => { dependency_error = Some(e); break; }
                    }
                }

                if let Some(err_msg) = dependency_error {
                    let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
                    return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"Dependency Error: {}"}}"#, err_msg.replace('"', "'")))).unwrap());
                }

                let sem_clone = gil_sem.clone();
                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                    let _permit = sem_clone.try_acquire().ok();
                    Python::with_gil(|py| -> PyResult<PyObject> {
                        
                        let kwargs = pyo3::types::PyDict::new_bound(py);
                        
                        for (k, v) in &path_params_c {
                            kwargs.set_item(k, v)?;
                        }
                        for (k, v) in resolved_args {
                            kwargs.set_item(k, v)?;
                        }

                        if let Some(req_name) = request_param_name {
                            let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, body: body_c.clone() })?;
                            kwargs.set_item(req_name, req_obj)?;
                        }

                        if let Some(ref model) = pydantic_model {
                            let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() }
                            else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
                            let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
                            if let Some(model_name) = pydantic_param_name {
                                kwargs.set_item(model_name, instance)?;
                            }
                        }

                        handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
                    })
                }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

                let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
                if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) {
                    r_headers.insert("Content-Type".to_string(), "application/json".to_string());
                }

                if !teardown_generators.is_empty() {
                    tokio::task::spawn_blocking(move || {
                        Python::with_gil(|py| {
                            if let Ok(builtins) = py.import_bound("builtins") {
                                for gen in teardown_generators {
                                    let _ = builtins.call_method1("next", (&gen,)); 
                                }
                            }
                        });
                    });
                }

                (r_status, r_body, r_headers)
            }
            None => {
                let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
                (404, r#"{"detail":"Not Found"}"#.to_string(), h)
            }
        }
    };

    let mut builder = HyperResponse::builder().status(status);
    for (k, v) in resp_headers { builder = builder.header(&k, &v); }

    println!("[INFO] {} {} - {} ({}ms)", method, path, status, start_time.elapsed().as_millis());
    Ok(builder.body(Body::from(resp_body)).unwrap())
}

async fn execute_python_handler(
    exec_res: PyResult<PyObject>, 
    is_async: bool, 
    serializer: &PyObject, 
    schedule_coro: &PyObject,
    raw_string: bool,
) -> (u16, String, HashMap<String, String>) {
    
    let py_result: PyResult<PyObject> = if is_async {
        match exec_res {
            Ok(coro) => {
                let (tx, rx) = oneshot::channel();
                let spawn_res = Python::with_gil(|py| -> PyResult<()> {
                    let cb = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) })?;
                    schedule_coro.bind(py).call1((coro, cb))?;
                    Ok(())
                });
                if let Err(e) = spawn_res { Err(e) } else {
                    match rx.await {
                        Ok(Ok(res)) => Ok(res),
                        Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))),
                        Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio channel dropped"))),
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
                if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
                    let resp_ref = resp.borrow();
                    let status = resp_ref.status_code;
                    let headers = resp_ref.headers.clone();
                    
                    let body_str = if raw_string {
                        if resp_ref.content.is_none(py) { String::new() }
                        else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() }
                        else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() }
                    } else {
                        serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default()
                    };
                    return (status, body_str, headers);
                }
                
                let body_str = if raw_string {
                    if py_obj.is_none(py) { String::new() }
                    else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() }
                    else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() }
                } else {
                    serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default()
                };
                (200, body_str, HashMap::new())
            }
            Err(err) => {
                let val = err.value_bound(py);
                
                if let Ok(pydantic) = py.import_bound("pydantic") {
                    if let Ok(val_err) = pydantic.getattr("ValidationError") {
                        if val.is_instance(&val_err).unwrap_or(false) {
                            if let Ok(errors) = val.call_method0("errors") {
                                let dict = PyDict::new_bound(py);
                                let _ = dict.set_item("detail", errors);
                                if let Ok(json) = py.import_bound("json") {
                                    if let Ok(dumps) = json.getattr("dumps") {
                                        if let Ok(err_str) = dumps.call1((dict,)) {
                                            return (422, err_str.extract().unwrap_or_default(), HashMap::new());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if val.hasattr("status_code").unwrap_or(false) && val.hasattr("detail").unwrap_or(false) {
                    let status_code: u16 = val.getattr("status_code").and_then(|v| v.extract()).unwrap_or(500);
                    let detail = val.getattr("detail").unwrap_or_else(|_| py.None().into_bound(py));
                    
                    let dict = PyDict::new_bound(py);
                    let _ = dict.set_item("detail", detail);
                    let err_str = py.import_bound("json").and_then(|j| j.getattr("dumps")).and_then(|d| d.call1((dict,)))
                        .and_then(|s| s.extract::<String>()).unwrap_or_else(|_| r#"{"detail":"Internal Error"}"#.to_string());
                        
                    let mut headers = HashMap::new();
                    if let Ok(h) = val.getattr("headers") {
                        if let Ok(h_dict) = h.downcast::<PyDict>() {
                            for (k, v) in h_dict {
                                if let (Ok(ks), Ok(vs)) = (k.extract::<String>(), v.extract::<String>()) {
                                    headers.insert(ks, vs);
                                }
                            }
                        }
                    }
                    return (status_code, err_str, headers);
                }
                
                (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
            }
        }
    })
}

#[pyclass]
struct RouteDecorator { routes: Routes, method: String, path: String }

#[allow(non_local_definitions)]
#[pymethods]
impl RouteDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let sig = inspect.call_method1("signature", (func.bind(py),))?;
        let params = sig.getattr("parameters")?;

        let mut pydantic_model = None;
        let mut pydantic_param_name = None;
        let mut request_schema_json = None;
        let mut request_param_name = None;
        let mut dependencies = Vec::new(); 

        if let Ok(params_dict) = params.call_method0("values") {
            if let Ok(iter) = params_dict.iter() {
                for p_res in iter {
                    if let Ok(p) = p_res {
                        let param_name: String = p.getattr("name")?.extract()?;

                        // Request object binding
                        if param_name == "req" || param_name == "request" {
                            request_param_name = Some(param_name);
                            continue;
                        }

                        // Pydantic model binding
                        if let Ok(annotation) = p.getattr("annotation") {
                            if annotation.hasattr("model_json_schema").unwrap_or(false) {
                                pydantic_model = Some(annotation.clone().into());
                                pydantic_param_name = Some(param_name.clone());
                                if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
                                    let json_mod = py.import_bound("json")?;
                                    if let Ok(schema_str) = json_mod.call_method1("dumps", (schema_dict,)) {
                                        if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); }
                                    }
                                }
                                continue; 
                            }
                        }

                        // Dependency binding via Python object inspection
                        if let Ok(default_val) = p.getattr("default") {
                            let depends_type = py.import_bound("rustapi.depends")?.getattr("Depends")?;
                            let is_depends = default_val.is_instance(&depends_type).unwrap_or(false);

                            if is_depends {
                                let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") {
                                    if explicit_dep.is_none() {
                                        p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py))
                                    } else {
                                        explicit_dep
                                    }
                                } else {
                                    py.None().into_bound(py)
                                };

                                if !dep_func.is_none() {
                                    let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                    let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                    let use_cache = default_val.getattr("use_cache")?.extract().unwrap_or(true);
                                    let dep_id = dep_func.as_ptr() as isize;

                                    dependencies.push(DependencyMeta {
                                        name: param_name.clone(),
                                        func: dep_func.into(),
                                        is_async: is_dep_async,
                                        is_generator: is_dep_gen,
                                        use_cache,
                                        id: dep_id,
                                    });
                                }
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
            is_async, 
            pydantic_model, 
            pydantic_param_name,
            request_schema_json,
            request_param_name,
            dependencies,
        });
        Ok(func)
    }
}

#[pyclass]
struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, description: Option<String> }

#[allow(non_local_definitions)]
#[pymethods]
impl ToolDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let fname: String = func.bind(py).getattr("__name__")?.extract()?;
        let doc: Option<String> = inspect.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or(None);
        
        let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
        let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
        let schema_json: serde_json::Value = serde_json::from_str(&schema_str).unwrap();

        self.tools.lock().unwrap().push(ToolEntry {
            name: self.name.clone().unwrap_or(fname), description: self.description.clone().or(doc).unwrap_or_default(),
            schema_json, handler: func.clone_ref(py), is_async,
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
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let doc: Option<String> = inspect.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or(None);

        self.resources.lock().unwrap().push(ResourceEntry {
            uri: self.uri.clone(), description: doc.unwrap_or_default(),
            mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), is_async,
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
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let fname: String = func.bind(py).getattr("__name__")?.extract()?;
        let doc: Option<String> = inspect.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or(None);

        self.prompts.lock().unwrap().push(PromptEntry {
            name: self.name.clone().unwrap_or(fname), description: self.description.clone().or(doc).unwrap_or_default(),
            handler: func.clone_ref(py), is_async,
        });
        Ok(func)
    }
}

#[pyfunction]
fn compute(py: Python<'_>, n: i64) -> PyResult<i64> {
    py.allow_threads(|| Ok((0..n).sum()))
}

#[pymodule]
fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<PyRequest>()?;
    m.add_class::<PyResponse>()?;
    m.add_function(wrap_pyfunction!(compute, m)?)?;
    Ok(())
}
