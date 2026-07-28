use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyBytes};
use std::collections::HashMap;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex as StdMutex};
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
    _request_schema_json: Option<String>,
    request_param_name: Option<String>,
    background_task_param_name: Option<String>,
    websocket_param_name: Option<String>,
    is_websocket: bool,
    dependencies: Vec<DependencyMeta>,
    param_names: Vec<String>,
    param_types: HashMap<String, ParamType>,
}

type Routes   = Arc<StdMutex<Vec<RouteEntry>>>;
type Handlers = Arc<StdMutex<Vec<(Py<PyAny>, bool)>>>;

struct ToolEntry     { name: String, _description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
struct PromptEntry   { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

type Tools     = Arc<StdMutex<Vec<ToolEntry>>>;
type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
type Prompts   = Arc<StdMutex<Vec<PromptEntry>>>;

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
                Segment::Param(s[1..s.len() - 1].to_string())
            } else {
                Segment::Literal(s.to_string())
            }
        })
        .collect()
}

fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
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
    for r in routes {
        if r.is_websocket { continue; }
        let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });
        if matches!(r.method.as_str(), "POST" | "PUT" | "PATCH") {
            if r.original_path.contains("upload") {
                method_obj["requestBody"] = json!({
                    "required": true,
                    "content": { "multipart/form-data": { "schema": { "type": "object", "properties": {
                        "document":    { "type": "string", "format": "binary" },
                        "description": { "type": "string" }
                    }}}}
                });
            } else {
                method_obj["requestBody"] = json!({
                    "required": true,
                    "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } }
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
    serde_json::to_string(&json!({
        "openapi": "3.0.0",
        "info": { "title": "RustAPI", "version": "0.1.0" },
        "paths": paths
    })).unwrap()
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
      window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' });
    };
  </script>
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
    fn read(&self, py: Python<'_>) -> PyObject {
        PyBytes::new_bound(py, &self.file_data).into()
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
    schedule_coro: &PyObject,
    raw_string: bool,
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
                if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
                    let resp_ref = resp.borrow();
                    let status  = resp_ref.status_code;
                    let headers = resp_ref.headers.clone();
                    let body_str = serialize_value(py, &resp_ref.content, serializer, raw_string);
                    return (status, body_str, headers);
                }
                // Plain return value.
                let body_str = serialize_value(py, &py_obj, serializer, raw_string);
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

#[pyclass]
struct Engine {
    routes: Routes,
    serializer: PyObject,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    schema_fn: PyObject,
    schedule_coro_fn: PyObject,
    startup_handlers: Handlers,
    shutdown_handlers: Handlers,
}

#[allow(non_local_definitions)]
#[pymethods]
impl Engine {
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
def _schema_from_signature(func):
    sig = inspect.signature(func)
    props = {name: {"type": "string"} for name in sig.parameters}
    return {"type": "object", "properties": props}
"#;
        let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
        Ok(Engine {
            routes:           Arc::new(StdMutex::new(Vec::new())),
            serializer:       module.getattr("_serialize_response")?.into(),
            schedule_coro_fn: module.getattr("_schedule_coro")?.into(),
            schema_fn:        module.getattr("_schema_from_signature")?.into(),
            tools:            Arc::new(StdMutex::new(Vec::new())),
            resources:        Arc::new(StdMutex::new(Vec::new())),
            prompts:          Arc::new(StdMutex::new(Vec::new())),
            startup_handlers:  Arc::new(StdMutex::new(Vec::new())),
            shutdown_handlers: Arc::new(StdMutex::new(Vec::new())),
        })
    }

    // -- Route decorators --------------------------------------------------
    fn get    (&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(),    path, is_ws: false } }
    fn post   (&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(),   path, is_ws: false } }
    fn put    (&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(),    path, is_ws: false } }
    fn delete (&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
    fn patch  (&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(),  path, is_ws: false } }
    fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(),  path, is_ws: true  } }

    /// Mount a sub-router, supporting all HTTP methods.
    #[pyo3(signature = (router, prefix = "".to_string()))]
    fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
        let routes: Vec<(String, String, Py<PyAny>)> = router.getattr(py, "routes")?.extract(py)?;
        for (method, path, func) in routes {
            let full_path = format!("{}{}", prefix, path).replace("//", "/");
            match method.as_str() {
                "GET"    => { self.get(full_path).__call__(py, func)?; }
                "POST"   => { self.post(full_path).__call__(py, func)?; }
                "PUT"    => { self.put(full_path).__call__(py, func)?; }
                "DELETE" => { self.delete(full_path).__call__(py, func)?; }
                "PATCH"  => { self.patch(full_path).__call__(py, func)?; }
                "WS"     => { self.websocket(full_path).__call__(py, func)?; }
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

            let exit_result: Result<(), PyErr> = py.allow_threads(move || {
                let spawn_children = || -> Vec<std::process::Child> {
                    (0..safe_workers)
                        .map(|i| Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap())
                        .collect()
                };

                let mut children = spawn_children();
                let (tx, rx) = std::sync::mpsc::channel();
                let _watcher = if reload {
                    let mut w = notify::recommended_watcher(tx).unwrap();
                    w.watch(Path::new("."), RecursiveMode::Recursive).unwrap();
                    Some(w)
                } else {
                    None
                };

                loop {
                    if reload {
                        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
                            if event.paths.iter().any(|p| p.extension().map_or(false, |e| e == "py")) {
                                for mut c in children { let _ = c.kill(); let _ = c.wait(); }
                                children = spawn_children();
                                continue;
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(250));
                    }
                    if let Err(e) = Python::with_gil(|py| py.check_signals()) {
                        for mut c in children { let _ = c.kill(); let _ = c.wait(); }
                        return Err(e);
                    }
                }
            });

            return match exit_result {
                Err(err) => Python::with_gil(|py| {
                    if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) }
                }),
                Ok(()) => Ok(()),
            };
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

        if !is_worker {
            eprintln!("INFO:     Started server process [{}]", std::process::id());
            eprintln!("INFO:     RustAPI server running on http://{host}:{port} (Press CTRL+C to quit)");
        }

        let routes           = self.routes.clone();
        let tools            = self.tools.clone();
        let resources        = self.resources.clone();
        let prompts          = self.prompts.clone();
        let serializer_arc   = Arc::new(self.serializer.clone_ref(py));
        let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
        let num_cpus         = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let gil_semaphore    = Arc::new(Semaphore::new(num_cpus * 2));
        let startup_handlers  = self.startup_handlers.clone();
        let shutdown_handlers = self.shutdown_handlers.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let mut shutdown_tx = Some(shutdown_tx);
        let (done_tx, done_rx) = mpsc::channel::<()>();

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
                let make_svc = make_service_fn(move |_| {
                    let (r, t, res, p, s, sc, sem) = (
                        routes.clone(), tools.clone(), resources.clone(), prompts.clone(),
                        serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone(),
                    );
                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| {
                            handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone())
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
    serializer: Arc<PyObject>,
    schedule_coro: Arc<PyObject>,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    gil_sem: Arc<Semaphore>,
) -> Result<HyperResponse<Body>, Infallible> {
    let method      = req.method().to_string();
    let path        = req.uri().path().to_string();
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
        let mut body_stream = req.into_body();
        while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
            let chunk = chunk_res.unwrap_or_default();
            if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE {
                return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap());
            }
            body_bytes.extend_from_slice(&chunk);
        }
    }
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // -----------------------------------------------------------------------
    // Route dispatch
    // -----------------------------------------------------------------------

    let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "text/html".to_string());
        (200u16, swagger_html(), h)

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
                    &routes, &serializer, &schedule_coro, &gil_sem,
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
                        execute_python_handler(exec_res, is_async_tool, serializer, schedule_coro, true).await;
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
    schedule_coro: &Arc<PyObject>,
    gil_sem: &Arc<Semaphore>,
) -> Result<HyperResponse<Body>, String> {
    // Extract route metadata.
    let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name,
         background_task_param_name, deps, param_names, param_types) =
        Python::with_gil(|py| {
            let guard = routes.lock().unwrap();
            let e = &guard[idx];
            (
                e.handler.clone_ref(py), e.is_async,
                e.pydantic_model.as_ref().map(|m| m.clone_ref(py)),
                e.pydantic_param_name.clone(), e.request_param_name.clone(),
                e.background_task_param_name.clone(),
                e.dependencies.clone(), e.param_names.clone(), e.param_types.clone(),
            )
        });

    // -- Dependency injection ---------------------------------------------
    let mut dependency_error: Option<String> = None;
    let mut resolved_args   = HashMap::<String, PyObject>::new();
    let mut dep_cache       = HashMap::<isize, PyObject>::new();
    let mut teardown_gens   = Vec::<PyObject>::new();

    for dep in deps {
        if dep.use_cache {
            if let Some(cached) = dep_cache.get(&dep.id) {
                let v = Python::with_gil(|py| cached.clone_ref(py));
                resolved_args.insert(dep.name.clone(), v);
                continue;
            }
        }

        let dep_res: Result<PyObject, String> = if dep._is_async {
            let coro = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
            match coro {
                Ok(c) => {
                    let (tx, rx) = oneshot::channel();
                    let sc = schedule_coro.clone();
                    Python::with_gil(|py| {
                        if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                            let _ = sc.bind(py).call1((c, cb));
                        }
                    });
                    match rx.await {
                        Ok(Ok(res))      => Ok(res),
                        Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())),
                        Err(_)           => Err("Asyncio dropped".to_string()),
                    }
                }
                Err(e) => Err(e),
            }
        } else {
            let sem      = gil_sem.clone();
            let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
            tokio::task::spawn_blocking(move || {
                let _permit = sem.try_acquire().ok();
                Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
            })
            .await
            .unwrap_or_else(|_| Err("Panic".to_string()))
        };

        match dep_res {
            Ok(obj) => {
                let val = Python::with_gil(|py| -> Result<PyObject, String> {
                    if dep.is_generator {
                        Ok(py.import_bound("builtins").unwrap().call_method1("next", (&obj,)).unwrap().into())
                    } else {
                        Ok(obj.clone_ref(py))
                    }
                });
                match val {
                    Ok(v) => {
                        if dep.is_generator { teardown_gens.push(obj); }
                        if dep.use_cache    { dep_cache.insert(dep.id, Python::with_gil(|py| v.clone_ref(py))); }
                        resolved_args.insert(dep.name, v);
                    }
                    Err(e) => { dependency_error = Some(e); break; }
                }
            }
            Err(e) => { dependency_error = Some(e); break; }
        }
    }

    if let Some(err_msg) = dependency_error {
        return Ok(
            HyperResponse::builder().status(500)
                .header("Content-Type", "application/json")
                .body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'"))))
                .unwrap()
        );
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
    let sem_c          = gil_sem.clone();
    let param_names_c  = param_names.clone();
    let path_params_c  = path_params.clone();
    let query_params_c = query_params.clone();
    let method_c       = method.clone();
    let path_c         = path.clone();
    let body_c         = body.clone();
    let headers_c      = headers_map.clone();
    let cookies_c      = cookies_map.clone();
    let form_c         = form_map.clone();
    let files_c        = files_map.clone();

    let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
        let _permit = sem_c.try_acquire().ok();
        Python::with_gil(|py| -> PyResult<PyObject> {
            let kwargs = pyo3::types::PyDict::new_bound(py);

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
        execute_python_handler(exec_res, is_async, serializer, schedule_coro, false).await;

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
struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

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
        let mut dependencies               = Vec::new();
        let mut param_names                = Vec::new();
        let mut param_types                = HashMap::new();

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

                    if let Ok(annotation) = p.getattr("annotation") {
                        if let Ok(name) = annotation.getattr("__name__") {
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
                        if annotation.hasattr("model_json_schema").unwrap_or(false) {
                            pydantic_model      = Some(annotation.clone().into());
                            pydantic_param_name = Some(param_name.clone());
                            if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
                                if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) {
                                    if let Ok(s) = schema_str.extract::<String>() {
                                        request_schema_json = Some(s);
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    if let Ok(default_val) = p.getattr("default") {
                        let is_depends = default_val
                            .getattr("__class__")
                            .and_then(|cls| cls.getattr("__name__"))
                            .and_then(|n| n.extract::<String>())
                            .map(|n| n == "Depends")
                            .unwrap_or(false);
                        if is_depends {
                            let dep_func = if let Ok(explicit) = default_val.getattr("dependency") {
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
                                    name: param_name, func: dep_func.into(),
                                    _is_async: is_dep_async, is_generator: is_dep_gen,
                                    use_cache: true, id: dep_id,
                                });
                            }
                        }
                    }
                }
            }
        }

        self.routes.lock().unwrap().push(RouteEntry {
            method: self.method.clone(), original_path: self.path.clone(),
            segments: parse_pattern(&self.path), handler: func.clone_ref(py), is_async,
            pydantic_model, pydantic_param_name, _request_schema_json: request_schema_json,
            request_param_name, background_task_param_name, websocket_param_name,
            is_websocket: self.is_ws, dependencies, param_names, param_types,
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
        let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
        let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        self.tools.lock().unwrap().push(ToolEntry {
            name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()),
            _description: self.description.clone().unwrap_or_default(),
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

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

#[pymodule]
fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<PyRequest>()?;
    m.add_class::<PyResponse>()?;
    m.add_class::<PyUploadFile>()?;
    m.add_class::<PyWebSocket>()?;
    m.add_class::<EventDecorator>()?;
    m.add_class::<PyStreamingResponse>()?;
    Ok(())
}
