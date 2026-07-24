// use pyo3::prelude::*;
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::fs;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex};
// use std::thread;
// use std::time::{Duration, Instant, SystemTime};

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server};
// use tokio::sync::oneshot;

// // ---------- Native File Watcher ----------

// fn get_latest_mtime(dir: &Path) -> SystemTime {
//     let mut max_time = SystemTime::UNIX_EPOCH;
//     if let Ok(entries) = fs::read_dir(dir) {
//         for entry in entries.flatten() {
//             let path = entry.path();
//             if path.is_dir() {
//                 if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
//                     if name.starts_with('.') || name == "__pycache__" || name == "venv" || name == "env" {
//                         continue;
//                     }
//                 }
//                 let dir_time = get_latest_mtime(&path);
//                 if dir_time > max_time {
//                     max_time = dir_time;
//                 }
//             } else if path.extension().and_then(|s| s.to_str()) == Some("py") {
//                 if let Ok(meta) = entry.metadata() {
//                     if let Ok(mtime) = meta.modified() {
//                         if mtime > max_time {
//                             max_time = mtime;
//                         }
//                     }
//                 }
//             }
//         }
//     }
//     max_time
// }

// // ---------- Route representation ----------

// #[derive(Clone)]
// enum Segment {
//     Literal(String),
//     Param(String),
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     param_count: usize,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     request_schema_json: Option<String>,
// }

// type Routes = Arc<Mutex<Vec<RouteEntry>>>;

// // ---------- MCP registries ----------
// // Same shape as RouteEntry/Routes above, but for MCP tools/resources/prompts.
// // Kept separate from the HTTP route table since they're dispatched via
// // JSON-RPC (POST /mcp) rather than path/method matching.

// struct ToolEntry {
//     name: String,
//     description: String,
//     schema: Option<Py<PyAny>>, // JSON-schema dict, built once at registration time
//     handler: Py<PyAny>,
//     is_async: bool,
// }

// struct ResourceEntry {
//     uri: String,
//     description: String,
//     mime_type: String,
//     handler: Py<PyAny>,
//     is_async: bool,
// }

// struct PromptEntry {
//     name: String,
//     description: String,
//     handler: Py<PyAny>,
//     is_async: bool,
// }

// type Tools = Arc<Mutex<Vec<ToolEntry>>>;
// type Resources = Arc<Mutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<Mutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> {
//     path.split('/').filter(|s| !s.is_empty()).collect()
// }

// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path)
//         .into_iter()
//         .map(|s| {
//             if s.starts_with('{') && s.ends_with('}') {
//                 Segment::Param(s[1..s.len() - 1].to_string())
//             } else {
//                 Segment::Literal(s.to_string())
//             }
//         })
//         .collect()
// }

// fn match_route(
//     routes: &[RouteEntry],
//     method: &str,
//     path: &str,
// ) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() {
//             continue;
//         }
//         let mut params = HashMap::new();
//         let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => {
//                     if l != val {
//                         ok = false;
//                         break;
//                     }
//                 }
//                 Segment::Param(name) => {
//                     params.insert(name.clone(), (*val).to_string());
//                 }
//             }
//         }
//         if ok {
//             return Some((idx, params));
//         }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = it.next().unwrap_or("");
//             let v = it.next().unwrap_or("");
//             let k = urlencoding::decode(k).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(v).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// // ---------- OpenAPI & Swagger Generators ----------

// fn extract_path_params(path: &str) -> Vec<String> {
//     let mut params = Vec::new();
//     for seg in path.split('/') {
//         if seg.starts_with('{') && seg.ends_with('}') {
//             params.push(seg[1..seg.len() - 1].to_string());
//         }
//     }
//     params
// }

// fn generate_openapi(routes: &[RouteEntry]) -> String {
//     let mut paths_map: HashMap<String, Vec<&RouteEntry>> = HashMap::new();
//     for r in routes {
//         paths_map
//             .entry(r.original_path.clone())
//             .or_default()
//             .push(r);
//     }

//     let mut paths_json = String::new();
//     for (path, entries) in paths_map {
//         let mut method_items = Vec::new();
//         for r in entries {
//             let method_lower = r.method.to_lowercase();
//             let path_params = extract_path_params(&path);
//             let mut parts = Vec::new();

//             if !path_params.is_empty() {
//                 let mut p_arr = String::from(r#""parameters":["#);
//                 for (i, p) in path_params.iter().enumerate() {
//                     if i > 0 { p_arr.push(','); }
//                     p_arr.push_str(&format!(
//                         r#"{{"name":"{}","in":"path","required":true,"schema":{{"type":"string"}}}}"#,
//                         p
//                     ));
//                 }
//                 p_arr.push(']');
//                 parts.push(p_arr);
//             }

//             if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
//                 let schema_json = r.request_schema_json.as_deref().unwrap_or(r#"{"type":"object","additionalProperties":true}"#);
//                 // Safe string concatenation to completely avoid format! brace conflicts
//                 let body_part = r#""requestBody":{"required":true,"content":{"application/json":{"schema":"#.to_string() 
//                     + schema_json 
//                     + "}}}";
//                 parts.push(body_part);
//             }

//             parts.push(r#""responses":{"200":{"description":"Successful Response"}}"#.to_string());

//             let method_json = format!(r#""{}":{{{}}}"#, method_lower, parts.join(","));
//             method_items.push(method_json);
//         }
//         paths_json.push_str(&format!(r#""{}":{{{}}},"#, path, method_items.join(",")));
//     }
//     if paths_json.ends_with(',') {
//         paths_json.pop();
//     }

//     format!(
//         r#"{{"openapi":"3.0.0","info":{{"title":"RustAPI","version":"0.1.0"}},"paths":{{{}}}}}"#,
//         paths_json
//     )
// }

// fn swagger_html() -> String {
//     r#"<!DOCTYPE html>
// <html lang="en">
// <head>
//   <meta charset="utf-8" />
//   <title>Swagger UI - RustAPI</title>
//   <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
// </head>
// <body>
//   <div id="swagger-ui"></div>
//   <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
//   <script>
//     window.onload = () => {
//       window.ui = SwaggerUIBundle({
//         url: '/openapi.json',
//         dom_id: '#swagger-ui',
//       });
//     };
//   </script>
// </body>
// </html>"#.to_string()
// }

// // ---------- Python-visible Request object ----------

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)]
//     method: String,
//     #[pyo3(get)]
//     path: String,
//     #[pyo3(get)]
//     path_params: HashMap<String, String>,
//     #[pyo3(get)]
//     query_params: HashMap<String, String>,
//     #[pyo3(get)]
//     body: String,
// }

// #[pymethods]
// impl PyRequest {
//     #[new]
//     fn new(
//         method: String,
//         path: String,
//         path_params: HashMap<String, String>,
//         query_params: HashMap<String, String>,
//         body: String,
//     ) -> Self {
//         PyRequest { method, path, path_params, query_params, body }
//     }

//     fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
//         let dict = pyo3::types::PyDict::new_bound(py);
//         dict.set_item("method", &self.method)?;
//         dict.set_item("path", &self.path)?;
        
//         let pp = pyo3::types::PyDict::new_bound(py);
//         for (k, v) in &self.path_params { pp.set_item(k, v)?; }
//         dict.set_item("path_params", pp)?;

//         let qp = pyo3::types::PyDict::new_bound(py);
//         for (k, v) in &self.query_params { qp.set_item(k, v)?; }
//         dict.set_item("query_params", qp)?;

//         dict.set_item("body", &self.body)?;
//         Ok(dict.into())
//     }

//     fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
//         let json = py.import_bound("json")?;
//         json.call_method1("loads", (&self.body,)).map(|v| v.into())
//     }
// }

// // ---------- Engine ----------

// #[pyclass]
// struct Engine {
//     routes: Routes,
//     asgi_handler: PyObject,
//     req_class: PyObject,
//     // Pre-compiled Python callable that serializes a handler's return value to a
//     // JSON string (falling back to `.model_dump()` for pydantic models). Built once
//     // at Engine construction time instead of being eval()'d from a hand-built string
//     // on every single request - faster, and immune to string-escaping mistakes.
//     serializer: PyObject,
//     // MCP (Model Context Protocol) registries. Same instance can serve both plain
//     // HTTP routes and an MCP server over Streamable HTTP at POST /mcp.
//     tools: Tools,
//     resources: Resources,
//     prompts: Prompts,
//     schema_fn: PyObject,   // builds a JSON-schema dict from a function's type hints
//     mcp_dispatch: PyObject, // async JSON-RPC dispatcher (initialize/tools/call/etc.)
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let asgi_code = r#"
// import asyncio
// import inspect
// import json
// import typing
// import urllib.parse

// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))

// _JSON_TYPE_MAP = {str: "string", int: "integer", float: "number", bool: "boolean", list: "array", dict: "object"}

// def _schema_from_signature(func):
//     # Builds a JSON-schema "object" describing a function's parameters, from its
//     # type hints. Used to auto-generate MCP tool/prompt input schemas so tools
//     # don't need a hand-written pydantic model just to be exposed over MCP.
//     sig = inspect.signature(func)
//     properties = {}
//     required = []
//     for name, param in sig.parameters.items():
//         ann = param.annotation
//         py_type = ann if ann is not inspect.Parameter.empty else str
//         optional = False
//         if typing.get_origin(py_type) is typing.Union:
//             args = [a for a in typing.get_args(py_type) if a is not type(None)]
//             if len(args) == 1:
//                 py_type = args[0]
//                 optional = True
//         json_type = _JSON_TYPE_MAP.get(py_type, "string")
//         prop = {"type": json_type}
//         if param.default is not inspect.Parameter.empty:
//             prop["default"] = param.default if isinstance(param.default, (str, int, float, bool, type(None))) else None
//         properties[name] = prop
//         if param.default is inspect.Parameter.empty and not optional:
//             required.append(name)
//     schema = {"type": "object", "properties": properties}
//     if required:
//         schema["required"] = required
//     return schema

// MCP_PROTOCOL_VERSION = "2025-06-18"

// async def handle_mcp_message(message, tools, resources, prompts):
//     msg_id = message.get("id")
//     method = message.get("method")
//     params = message.get("params") or {}

//     def ok(result):
//         return {"jsonrpc": "2.0", "id": msg_id, "result": result}

//     def err(code, msg):
//         return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}

//     if method == "initialize":
//         return ok({
//             "protocolVersion": MCP_PROTOCOL_VERSION,
//             "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
//             "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"},
//         })

//     if method in ("notifications/initialized", "initialized"):
//         return None

//     if method == "ping":
//         return ok({})

//     if method == "tools/list":
//         items = [
//             {"name": name, "description": meta.get("description") or "", "inputSchema": meta.get("schema") or {"type": "object", "properties": {}}}
//             for name, meta in tools.items()
//         ]
//         return ok({"tools": items})

//     if method == "tools/call":
//         name = params.get("name")
//         arguments = params.get("arguments") or {}
//         meta = tools.get(name)
//         if meta is None:
//             return err(-32602, f"Unknown tool: {name}")
//         handler = meta["handler"]
//         try:
//             if meta["is_async"]:
//                 result = await handler(**arguments)
//             else:
//                 result = await asyncio.to_thread(handler, **arguments)
//             text = result if isinstance(result, str) else _serialize_response(result)
//             return ok({"content": [{"type": "text", "text": text}], "isError": False})
//         except Exception as e:
//             return ok({"content": [{"type": "text", "text": str(e)}], "isError": True})

//     if method == "resources/list":
//         items = [
//             {"uri": uri, "name": meta.get("description") or uri, "mimeType": meta.get("mime_type") or "text/plain"}
//             for uri, meta in resources.items()
//         ]
//         return ok({"resources": items})

//     if method == "resources/read":
//         uri = params.get("uri")
//         meta = resources.get(uri)
//         if meta is None:
//             return err(-32602, f"Unknown resource: {uri}")
//         handler = meta["handler"]
//         result = await handler() if meta["is_async"] else await asyncio.to_thread(handler)
//         text = result if isinstance(result, str) else _serialize_response(result)
//         return ok({"contents": [{"uri": uri, "mimeType": meta.get("mime_type") or "text/plain", "text": text}]})

//     if method == "prompts/list":
//         items = [{"name": name, "description": meta.get("description") or "", "arguments": []} for name, meta in prompts.items()]
//         return ok({"prompts": items})

//     if method == "prompts/get":
//         name = params.get("name")
//         arguments = params.get("arguments") or {}
//         meta = prompts.get(name)
//         if meta is None:
//             return err(-32602, f"Unknown prompt: {name}")
//         handler = meta["handler"]
//         text = await handler(**arguments) if meta["is_async"] else await asyncio.to_thread(handler, **arguments)
//         return ok({"messages": [{"role": "user", "content": {"type": "text", "text": text}}]})

//     if msg_id is None:
//         return None  # unrecognized notification - do not respond

//     return err(-32601, f"Method not found: {method}")

// async def asgi_app(engine, req_class, scope, receive, send):
//     if scope["type"] != "http":
//         return

//     method = scope["method"]
//     path = scope.get("path", "/")

//     if method == "GET" and path == "/docs":
//         html = engine._swagger_html()
//         await send({"type": "http.response.start", "status": 200, "headers": [[b"content-type", b"text/html"]]})
//         await send({"type": "http.response.body", "body": html.encode("utf-8")})
//         return
//     elif method == "GET" and path == "/openapi.json":
//         spec = engine._openapi_spec()
//         await send({"type": "http.response.start", "status": 200, "headers": [[b"content-type", b"application/json"]]})
//         await send({"type": "http.response.body", "body": spec.encode("utf-8")})
//         return

//     match = engine._dispatch(method, path)
//     if match:
//         handler, param_count, is_async, path_params, pydantic_model = match
        
//         body_bytes = b""
//         more_body = True
//         while more_body:
//             message = await receive()
//             body_bytes += message.get("body", b"")
//             more_body = message.get("more_body", False)
            
//         body_str = body_bytes.decode("utf-8", errors="replace")
        
//         query_string = scope.get("query_string", b"").decode("utf-8")
//         query_params = {}
//         if query_string:
//             for pair in query_string.split("&"):
//                 if "=" in pair:
//                     k, v = pair.split("=", 1)
//                     query_params[urllib.parse.unquote_plus(k)] = urllib.parse.unquote_plus(v)
        
//         args = ()
//         if param_count > 0:
//             if pydantic_model is not None:
//                 try:
//                     body_data = json.loads(body_str) if body_str else {}
//                 except Exception:
//                     body_data = {}
//                 arg = pydantic_model.model_validate(body_data)
//                 args = (arg,)
//             else:
//                 req = req_class(method, path, path_params, query_params, body_str)
//                 args = (req,)
            
//         try:
//             if is_async:
//                 res = await handler(*args)
//             else:
//                 res = await asyncio.to_thread(handler, *args)
            
//             body_res = _serialize_response(res).encode("utf-8")
//             status = 200
//         except Exception as e:
//             res = {"error": f"Internal Server Error: {str(e)}"}
//             body_res = json.dumps(res).encode("utf-8")
//             status = 500
//     else:
//         res = {"error": "Not Found"}
//         body_res = json.dumps(res).encode("utf-8")
//         status = 404
        
//     await send({"type": "http.response.start", "status": status, "headers": [[b"content-type", b"application/json"]]})
//     await send({"type": "http.response.body", "body": body_res})
// "#;
//         let module = PyModule::from_code_bound(py, asgi_code, "asgi_internal.py", "asgi_internal")?;
//         let asgi_handler = module.getattr("asgi_app")?.into();
//         let req_class = py.get_type_bound::<PyRequest>().into();
//         let serializer = module.getattr("_serialize_response")?.into();
//         let schema_fn = module.getattr("_schema_from_signature")?.into();
//         let mcp_dispatch = module.getattr("handle_mcp_message")?.into();

//         Ok(Engine {
//             routes: Arc::new(Mutex::new(Vec::new())),
//             asgi_handler,
//             req_class,
//             serializer,
//             tools: Arc::new(Mutex::new(Vec::new())),
//             resources: Arc::new(Mutex::new(Vec::new())),
//             prompts: Arc::new(Mutex::new(Vec::new())),
//             schema_fn,
//             mcp_dispatch,
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path } }

//     // ---- MCP: same Engine instance can expose tools/resources/prompts over
//     // JSON-RPC at POST /mcp, alongside its normal HTTP routes. ----

//     #[pyo3(signature = (name=None, description=None))]
//     fn tool(&self, py: Python<'_>, name: Option<String>, description: Option<String>) -> ToolDecorator {
//         ToolDecorator { tools: self.tools.clone(), schema_fn: self.schema_fn.clone_ref(py), name, description }
//     }

//     #[pyo3(signature = (uri, mime_type=None))]
//     fn resource(&self, uri: String, mime_type: Option<String>) -> ResourceDecorator {
//         ResourceDecorator { resources: self.resources.clone(), uri, mime_type }
//     }

//     #[pyo3(signature = (name=None, description=None))]
//     fn prompt(&self, name: Option<String>, description: Option<String>) -> PromptDecorator {
//         PromptDecorator { prompts: self.prompts.clone(), name, description }
//     }

//     #[pyo3(signature = (scope, receive, send))]
//     fn __call__<'py>(
//         slf: &Bound<'py, Self>, 
//         py: Python<'py>, 
//         scope: PyObject, 
//         receive: PyObject, 
//         send: PyObject
//     ) -> PyResult<Bound<'py, PyAny>> {
//         let engine = slf.borrow();
//         engine.asgi_handler.bind(py).call1((slf.clone(), engine.req_class.clone_ref(py), scope, receive, send))
//     }

//     fn _dispatch(&self, py: Python<'_>, method: String, path: String) -> PyResult<Option<(PyObject, usize, bool, HashMap<String, String>, Option<PyObject>)>> {
//         let guard = self.routes.lock().unwrap();
//         match match_route(&guard, &method, &path) {
//             Some((idx, path_params)) => {
//                 let entry = &guard[idx];
//                 let model_obj = entry.pydantic_model.as_ref().map(|m| m.clone_ref(py));
//                 Ok(Some((entry.handler.clone_ref(py), entry.param_count, entry.is_async, path_params, model_obj)))
//             }
//             None => Ok(None),
//         }
//     }

//     fn _swagger_html(&self) -> String { swagger_html() }
//     fn _openapi_spec(&self) -> String {
//         let guard = self.routes.lock().unwrap();
//         generate_openapi(&guard)
//     }

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();

//         if reload && !is_worker {
//             println!("👀 Auto-reload is enabled. Watching for .py file changes...");

//             let sys = py.import_bound("sys")?;
//             let executable: String = sys.getattr("executable")?.extract()?;
//             let argv: Vec<String> = sys.getattr("argv")?.extract()?;

//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let mut child = Command::new(&executable)
//                     .args(&argv)
//                     .env("RUSTAPI_WORKER", "1")
//                     .spawn()
//                     .expect("Failed to start worker process");

//                 let root_dir = Path::new(".");
//                 let mut last_mtime = get_latest_mtime(root_dir);

//                 loop {
//                     let current_mtime = get_latest_mtime(root_dir);
//                     if current_mtime > last_mtime {
//                         println!("🔄 File change detected! Restarting server...\n");
//                         let _ = child.kill();
//                         let _ = child.wait();

//                         child = Command::new(&executable)
//                             .args(&argv)
//                             .env("RUSTAPI_WORKER", "1")
//                             .spawn()
//                             .expect("Failed to restart worker process");

//                         last_mtime = current_mtime;
//                     }

//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) {
//                         let _ = child.kill();
//                         let _ = child.wait();
//                         return Err(e);
//                     }
//                     thread::sleep(Duration::from_millis(250));
//                 }
//             });

//             if let Err(err) = exit_result {
//                 return Python::with_gil(|py| {
//                     if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
//                         println!("\n[INFO] Server shut down successfully.");
//                         Ok(())
//                     } else {
//                         Err(err)
//                     }
//                 });
//             }
//             return Ok(());
//         }

//         let routes = self.routes.clone();
//         let serializer = self.serializer.clone_ref(py);
//         let tools = self.tools.clone();
//         let resources = self.resources.clone();
//         let prompts = self.prompts.clone();
//         let mcp_dispatch = self.mcp_dispatch.clone_ref(py);
//         let addr: SocketAddr = format!("{host}:{port}")
//             .parse()
//             .map_err(|e: std::net::AddrParseError| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

//         if !is_worker {
//             println!("🚀 rustapi listening on http://{addr}");
//         } else {
//             println!("🚀 Worker started. Listening on http://{addr}");
//         }
//         println!("📄 Swagger UI docs available at http://{addr}/docs");
//         println!("Press Ctrl+C to stop the server.");

//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
//         let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_conn| {
//                     let routes = routes.clone();
//                     let serializer = serializer.clone();
//                     let tools = tools.clone();
//                     let resources = resources.clone();
//                     let prompts = prompts.clone();
//                     let mcp_dispatch = mcp_dispatch.clone();
//                     async move {
//                         Ok::<_, Infallible>(service_fn(move |req| handle(
//                             req,
//                             routes.clone(),
//                             serializer.clone(),
//                             tools.clone(),
//                             resources.clone(),
//                             prompts.clone(),
//                             mcp_dispatch.clone(),
//                         )))
//                     }
//                 });

//                 let server = Server::bind(&addr).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async {
//                     let _ = shutdown_rx.await;
//                 });

//                 if let Err(e) = graceful.await {
//                     eprintln!("Server error: {e}");
//                 }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() {
//                     return None;
//                 }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) {
//                     let _ = shutdown_tx.send(());
//                     return Some(err);
//                 }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });

//         let _ = server_handle.join();
//         if let Some(err) = pending_err {
//             Python::with_gil(|py| {
//                 if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
//                     if !is_worker {
//                         println!("\n[INFO] Server shut down successfully.");
//                     }
//                     Ok(())
//                 } else {
//                     Err(err)
//                 }
//             })
//         } else {
//             Ok(())
//         }
//     }
// }

// async fn handle(
//     req: HyperRequest<Body>,
//     routes: Routes,
//     serializer: PyObject,
//     tools: Tools,
//     resources: Resources,
//     prompts: Prompts,
//     mcp_dispatch: PyObject,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let start_time = Instant::now();
//     let method = req.method().to_string();
//     let path = req.uri().path().to_string();
//     let query_params = parse_query(req.uri().query());

//     let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, content_type) = if method == "GET" && path == "/docs" {
//         (200, swagger_html(), "text/html")
//     } else if method == "GET" && path == "/openapi.json" {
//         let spec = {
//             let guard = routes.lock().unwrap();
//             generate_openapi(&guard)
//         };
//         (200, spec, "application/json")
//     } else if method == "POST" && path == "/mcp" {
//         // MCP Streamable HTTP transport (simplified): one JSON-RPC message in,
//         // one JSON response out. No SSE/server-push in this version.
//         let outcome = tokio::task::spawn_blocking(move || {
//             Python::with_gil(|py| -> Result<(u16, String), PyErr> {
//                 let json_mod = py.import_bound("json")?;
//                 let message = json_mod.call_method1("loads", (&body,))?;

//                 let tools_dict = pyo3::types::PyDict::new_bound(py);
//                 for t in tools.lock().unwrap().iter() {
//                     let meta = pyo3::types::PyDict::new_bound(py);
//                     meta.set_item("handler", t.handler.clone_ref(py))?;
//                     meta.set_item("is_async", t.is_async)?;
//                     meta.set_item("schema", t.schema.as_ref().map(|s| s.clone_ref(py)))?;
//                     meta.set_item("description", &t.description)?;
//                     tools_dict.set_item(&t.name, meta)?;
//                 }

//                 let resources_dict = pyo3::types::PyDict::new_bound(py);
//                 for r in resources.lock().unwrap().iter() {
//                     let meta = pyo3::types::PyDict::new_bound(py);
//                     meta.set_item("handler", r.handler.clone_ref(py))?;
//                     meta.set_item("is_async", r.is_async)?;
//                     meta.set_item("description", &r.description)?;
//                     meta.set_item("mime_type", &r.mime_type)?;
//                     resources_dict.set_item(&r.uri, meta)?;
//                 }

//                 let prompts_dict = pyo3::types::PyDict::new_bound(py);
//                 for p in prompts.lock().unwrap().iter() {
//                     let meta = pyo3::types::PyDict::new_bound(py);
//                     meta.set_item("handler", p.handler.clone_ref(py))?;
//                     meta.set_item("is_async", p.is_async)?;
//                     meta.set_item("description", &p.description)?;
//                     prompts_dict.set_item(&p.name, meta)?;
//                 }

//                 let coro = mcp_dispatch.bind(py).call1((message, tools_dict, resources_dict, prompts_dict))?;
//                 let asyncio = py.import_bound("asyncio")?;
//                 let result = asyncio.call_method1("run", (coro,))?;

//                 if result.is_none() {
//                     // JSON-RPC notification - no response body per spec.
//                     Ok((202, String::new()))
//                 } else {
//                     let serialized: String = serializer.bind(py).call1((result,))?.extract()?;
//                     Ok((200, serialized))
//                 }
//             })
//         }).await;

//         match outcome {
//             Ok(Ok((s, b))) => (s, b, "application/json"),
//             Ok(Err(e)) => (500, format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'")), "application/json"),
//             Err(_) => (500, r#"{"error":"Rust background task panicked"}"#.to_string(), "application/json"),
//         }
//     } else {
//         let matched = {
//             let guard = routes.lock().unwrap();
//             match_route(&guard, &method, &path)
//         };

//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, param_count, is_async, pydantic_model) = {
//                     let guard = routes.lock().unwrap();
//                     let entry = &guard[idx];
//                     Python::with_gil(|py| (
//                         entry.handler.clone_ref(py), 
//                         entry.param_count, 
//                         entry.is_async, 
//                         entry.pydantic_model.as_ref().map(|m| m.clone_ref(py))
//                     ))
//                 };

//                 let method2 = method.clone();
//                 let path2 = path.clone();

//                 let outcome = tokio::task::spawn_blocking(move || {
//                     Python::with_gil(|py| -> Result<(u16, String), PyErr> {
//                         let result = if param_count == 0 {
//                             handler.call0(py)?
//                         } else if let Some(ref model) = pydantic_model {
//                             let json_mod = py.import_bound("json")?;
//                             let py_dict = if body.is_empty() {
//                                 pyo3::types::PyDict::new_bound(py).into_any()
//                             } else {
//                                 json_mod.call_method1("loads", (&body,))?.into_any()
//                             };
//                             let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
//                             handler.call1(py, (instance,))?
//                         } else {
//                             let req_obj = Py::new(py, PyRequest { 
//                                 method: method2, path: path2, path_params, query_params, body 
//                             })?;
//                             handler.call1(py, (req_obj,))?
//                         };

//                         let val = if is_async {
//                             let asyncio = py.import_bound("asyncio")?;
//                             asyncio.call_method1("run", (result,))?
//                         } else {
//                             result.into_bound(py)
//                         };

//                         // Call the pre-compiled Python serializer function instead of
//                         // eval()-ing a hand-built code string on every request.
//                         let serialized: String = serializer.bind(py).call1((val,))?.extract()?;

//                         Ok((200, serialized))
//                     })
//                 }).await;

//                 match outcome {
//                     Ok(Ok((s, b))) => (s, b, "application/json"),
//                     Ok(Err(e)) => (500, format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'")), "application/json"),
//                     Err(_) => (500, r#"{"error":"Rust background task panicked"}"#.to_string(), "application/json"),
//                 }
//             }
//             None => (404, r#"{"error":"not found"}"#.to_string(), "application/json"),
//         }
//     };

//     let duration = start_time.elapsed().as_millis();
//     println!("[INFO] {} {} - {} ({}ms)", method, path, status, duration);

//     Ok(HyperResponse::builder()
//         .status(status)
//         .header("Content-Type", content_type)
//         .body(Body::from(resp_body))
//         .unwrap())
// }

// #[pyclass]
// struct RouteDecorator {
//     routes: Routes,
//     method: String,
//     path: String,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?;
        
//         let iscoroutinefunction = inspect.getattr("iscoroutinefunction")?;
//         let is_async: bool = iscoroutinefunction.call1((func.bind(py),))?.extract()?;

//         let sig = inspect.call_method1("signature", (func.bind(py),))?;
//         let params = sig.getattr("parameters")?;
//         let param_count: usize = params.call_method0("__len__")?.extract()?;

//         let mut pydantic_model: Option<Py<PyAny>> = None;
//         let mut request_schema_json: Option<String> = None;

//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         if let Ok(annotation) = p.getattr("annotation") {
//                             if annotation.hasattr("model_json_schema").unwrap_or(false) {
//                                 pydantic_model = Some(annotation.clone().into());
//                                 if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
//                                     if let Ok(json_mod) = py.import_bound("json") {
//                                         if let Ok(schema_str) = json_mod.call_method1("dumps", (schema_dict,)) {
//                                             if let Ok(s) = schema_str.extract::<String>() {
//                                                 request_schema_json = Some(s);
//                                             }
//                                         }
//                                     }
//                                 }
//                                 break;
//                             }
//                         }
//                     }
//                 }
//             }
//         }

//         self.routes.lock().unwrap().push(RouteEntry {
//             method: self.method.clone(),
//             original_path: self.path.clone(),
//             segments: parse_pattern(&self.path),
//             handler: func.clone_ref(py),
//             param_count,
//             is_async,
//             pydantic_model,
//             request_schema_json,
//         });
//         Ok(func)
//     }
// }

// // ---------- MCP decorators ----------
// // Mirror RouteDecorator above: Engine.tool()/.resource()/.prompt() return one of
// // these, which registers the wrapped function on __call__ and hands it straight
// // back unmodified (so it stays a normal, directly-callable Python function).

// #[pyclass]
// struct ToolDecorator {
//     tools: Tools,
//     schema_fn: PyObject,
//     name: Option<String>,
//     description: Option<String>,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl ToolDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?;
//         let is_async: bool = inspect
//             .getattr("iscoroutinefunction")?
//             .call1((func.bind(py),))?
//             .extract()?;
//         let fname: String = func.bind(py).getattr("__name__")?.extract()?;
//         let doc: Option<String> = inspect
//             .call_method1("getdoc", (func.bind(py),))?
//             .extract()
//             .unwrap_or(None);

//         let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;

//         self.tools.lock().unwrap().push(ToolEntry {
//             name: self.name.clone().unwrap_or(fname),
//             description: self.description.clone().or(doc).unwrap_or_default(),
//             schema: Some(schema_obj.into()),
//             handler: func.clone_ref(py),
//             is_async,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ResourceDecorator {
//     resources: Resources,
//     uri: String,
//     mime_type: Option<String>,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl ResourceDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?;
//         let is_async: bool = inspect
//             .getattr("iscoroutinefunction")?
//             .call1((func.bind(py),))?
//             .extract()?;
//         let doc: Option<String> = inspect
//             .call_method1("getdoc", (func.bind(py),))?
//             .extract()
//             .unwrap_or(None);

//         self.resources.lock().unwrap().push(ResourceEntry {
//             uri: self.uri.clone(),
//             description: doc.unwrap_or_default(),
//             mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()),
//             handler: func.clone_ref(py),
//             is_async,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct PromptDecorator {
//     prompts: Prompts,
//     name: Option<String>,
//     description: Option<String>,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl PromptDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?;
//         let is_async: bool = inspect
//             .getattr("iscoroutinefunction")?
//             .call1((func.bind(py),))?
//             .extract()?;
//         let fname: String = func.bind(py).getattr("__name__")?.extract()?;
//         let doc: Option<String> = inspect
//             .call_method1("getdoc", (func.bind(py),))?
//             .extract()
//             .unwrap_or(None);

//         self.prompts.lock().unwrap().push(PromptEntry {
//             name: self.name.clone().unwrap_or(fname),
//             description: self.description.clone().or(doc).unwrap_or_default(),
//             handler: func.clone_ref(py),
//             is_async,
//         });
//         Ok(func)
//     }
// }

// #[rustapi::tool]
// fn compute(py: Python<'_>, n: i64) -> PyResult<i64> {
//     py.allow_threads(|| (0..n).sum())
// }

// #[pymodule]
// fn rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?;
//     m.add_class::<PyRequest>()?;
//     Ok(())
// }






use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3::types::{PyDict, PyString};
use std::collections::{HashMap, HashSet};
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
use tokio::sync::oneshot;

const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// ---------- Route representation ----------

#[derive(Clone)]
enum Segment {
    Literal(String),
    Param(String),
}

#[derive(Clone, PartialEq)]
enum ParamKind {
    Path,
    Query,
    Body,
    Request,
}

/// Metadata for a single handler parameter, computed once at route-registration
/// time so the hot request path never has to re-inspect the function signature.
// REMOVED: #[derive(Clone)] - We implement a manual clone_ref instead
struct ParamMeta {
    name: String,
    kind: ParamKind,
    /// The "effective" python type (Optional[X] unwrapped to X). None for Request/Body kinds.
    effective_type: Py<PyAny>,
    /// OpenAPI type name ("string"/"integer"/"number"/"boolean") - unused for Body/Request.
    #[allow(dead_code)]
    type_name: String,
    /// Whether this parameter is required (no default value / not Optional).
    required: bool,
    default: Py<PyAny>,
    /// Present only for ParamKind::Body - the pydantic model class.
    pydantic_model: Option<Py<PyAny>>,
}

// FIXED: Manual clone implementation that respects the PyO3 GIL
impl ParamMeta {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        ParamMeta {
            name: self.name.clone(),
            kind: self.kind.clone(),
            effective_type: self.effective_type.clone_ref(py),
            type_name: self.type_name.clone(),
            required: self.required,
            default: self.default.clone_ref(py),
            pydantic_model: self.pydantic_model.as_ref().map(|m| m.clone_ref(py)),
        }
    }
}

struct RouteEntry {
    method: String,
    original_path: String,
    segments: Vec<Segment>,
    handler: Py<PyAny>,
    is_async: bool,
    params: Vec<ParamMeta>,
    /// Cached JSON schema string for the body param (if any), for OpenAPI generation.
    body_schema_json: Option<String>,
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
                    // Path params must be URL-decoded, same as query params.
                    let decoded = urlencoding::decode(val).map(|c| c.into_owned()).unwrap_or_else(|_| (*val).to_string());
                    params.insert(name.clone(), decoded);
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
            let v_raw = it.next().unwrap_or("").replace('+', " ");
            let v = urlencoding::decode(&v_raw).unwrap_or_default().into_owned();
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
        let mut method_obj = json!({ "responses": {
            "200": { "description": "Successful Response" },
            "422": { "description": "Validation Error" }
        } });

        let mut oas_params = Vec::new();
        for p in &r.params {
            let (loc, required) = match p.kind {
                ParamKind::Path => ("path", true),
                ParamKind::Query => ("query", p.required),
                _ => continue,
            };
            oas_params.push(json!({
                "name": p.name,
                "in": loc,
                "required": required,
                "schema": { "type": p.type_name }
            }));
        }
        if !oas_params.is_empty() {
            method_obj["parameters"] = json!(oas_params);
        }

        if let Some(schema_str) = &r.body_schema_json {
            let schema: serde_json::Value = serde_json::from_str(schema_str)
                .unwrap_or_else(|_| json!({"type": "object", "additionalProperties": true}));
            method_obj["requestBody"] = json!({
                "required": true,
                "content": { "application/json": { "schema": schema } }
            });
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
    #[pyo3(get)] body: String,
}

#[pymethods]
impl PyRequest {
    #[new]
    fn new(method: String, path: String, path_params: HashMap<String, String>, query_params: HashMap<String, String>, body: String) -> Self {
        PyRequest { method, path, path_params, query_params, body }
    }

    fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        py.import_bound("json")?.call_method1("loads", (&self.body,)).map(|v| v.into())
    }
}

#[pyclass]
struct CoroCallback {
    tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, String>>>>,
}

#[pymethods]
impl CoroCallback {
    #[pyo3(signature = (result, error))]
    fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            if error.is_none(py) {
                let _ = tx.send(Ok(result));
            } else {
                let _ = tx.send(Err(error.to_string()));
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
    effective_type_fn: PyObject,
    type_name_fn: PyObject,
    convert_value_fn: PyObject,
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
            callback(None, str(e))
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

def _effective_type(annotation):
    """Unwraps Optional[X] -> X. Falls back to str for missing/complex annotations."""
    if annotation is inspect.Parameter.empty:
        return str
    origin = typing.get_origin(annotation)
    if origin is typing.Union:
        args = [a for a in typing.get_args(annotation) if a is not type(None)]
        if len(args) == 1:
            return args[0]
    if annotation in (str, int, float, bool):
        return annotation
    return str

def _type_name(t):
    """Maps a python scalar type to its OpenAPI schema type name."""
    if t is int:
        return "integer"
    if t is float:
        return "number"
    if t is bool:
        return "boolean"
    return "string"

def _convert_value(value, t):
    """Coerces a raw string (path/query param) into the target python type.
    Raises ValueError/TypeError on failure, which the Rust side turns into a 422."""
    if t is bool:
        if isinstance(value, bool):
            return value
        v = str(value).strip().lower()
        if v in ("1", "true", "yes", "on"):
            return True
        if v in ("0", "false", "no", "off", ""):
            return False
        raise ValueError(f"invalid boolean value: {value!r}")
    if t is int:
        return int(value)
    if t is float:
        return float(value)
    return value
"#;
        let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;

        Ok(Engine {
            routes: Arc::new(Mutex::new(Vec::new())),
            serializer: module.getattr("_serialize_response")?.into(),
            schedule_coro_fn: module.getattr("_schedule_coro")?.into(),
            schema_fn: module.getattr("_schema_from_signature")?.into(),
            effective_type_fn: module.getattr("_effective_type")?.into(),
            type_name_fn: module.getattr("_type_name")?.into(),
            convert_value_fn: module.getattr("_convert_value")?.into(),
            tools: Arc::new(Mutex::new(Vec::new())),
            resources: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn get(&self, py: Python<'_>, path: String) -> RouteDecorator { self.route_decorator(py, "GET", path) }
    fn post(&self, py: Python<'_>, path: String) -> RouteDecorator { self.route_decorator(py, "POST", path) }
    fn put(&self, py: Python<'_>, path: String) -> RouteDecorator { self.route_decorator(py, "PUT", path) }
    fn patch(&self, py: Python<'_>, path: String) -> RouteDecorator { self.route_decorator(py, "PATCH", path) }
    fn delete(&self, py: Python<'_>, path: String) -> RouteDecorator { self.route_decorator(py, "DELETE", path) }

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

        // ==========================================
        // 1. MASTER PROCESS MANAGER (Scaling & Reload)
        // ==========================================
        if (reload || safe_workers > 1) && !is_worker {
            println!("🚀 Starting Master process (PID {}) spanning {} worker(s)...", std::process::id(), safe_workers);
            if reload {
                println!("👀 Auto-reload enabled. Watching for .py file changes...");
            }

            let sys = py.import_bound("sys")?;
            let executable: String = sys.getattr("executable")?.extract()?;
            let argv: Vec<String> = sys.getattr("argv")?.extract()?;

            let exit_result: Result<(), PyErr> = py.allow_threads(move || {
                // FIXED: Removed unnecessary 'mut' and unused assignment warnings
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
                    // Handle file reloads
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

                    // Handle Ctrl+C (Graceful Shutdown)
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

        // ==========================================
        // 2. WORKER PROCESS (HTTP Server)
        // ==========================================
        let worker_id = std::env::var("RUSTAPI_WORKER").unwrap_or_else(|_| "0".to_string());

        let routes = self.routes.clone();
        let tools = self.tools.clone();
        let resources = self.resources.clone();
        let prompts = self.prompts.clone();
        let serializer_arc = Arc::new(self.serializer.clone_ref(py));
        let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
        let convert_value_arc = Arc::new(self.convert_value_fn.clone_ref(py));

        let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();

        // Advanced OS-level socket configuration (SO_REUSEPORT)
        let domain = if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 };
        let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None).unwrap();
        socket.set_reuse_address(true).unwrap();
        #[cfg(unix)] // macOS and Linux strictly require this to share ports
        socket.set_reuse_port(true).unwrap();

        socket.bind(&addr.into()).unwrap();
        socket.listen(1024).unwrap();

        let std_listener: std::net::TcpListener = socket.into();
        std_listener.set_nonblocking(true).unwrap(); // Required for Tokio conversion

        if worker_id == "0" {
            println!("🚀 rustapi listening on http://{addr}");
        }

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<()>();

        let server_handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
            rt.block_on(async move {
                let make_svc = make_service_fn(move |_conn| {
                    let routes = routes.clone();
                    let tools = tools.clone();
                    let resources = resources.clone();
                    let prompts = prompts.clone();
                    let serializer = serializer_arc.clone();
                    let schedule_coro = schedule_coro_arc.clone();
                    let convert_value = convert_value_arc.clone();

                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| handle(
                            req, routes.clone(), serializer.clone(), schedule_coro.clone(),
                            tools.clone(), resources.clone(), prompts.clone(), convert_value.clone(),
                        )))
                    }
                });

                // Feed our specialized SO_REUSEPORT socket into Hyper
                let server = Server::from_tcp(std_listener).unwrap().serve(make_svc);
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

impl Engine {
    fn route_decorator(&self, py: Python<'_>, method: &str, path: String) -> RouteDecorator {
        RouteDecorator {
            routes: self.routes.clone(),
            method: method.to_string(),
            path,
            effective_type_fn: self.effective_type_fn.clone_ref(py),
            type_name_fn: self.type_name_fn.clone_ref(py),
        }
    }
}

/// Error produced while binding/validating a request against a route's declared
/// parameters, versus an error raised by the user's own business logic.
enum DispatchError {
    /// A 422-worthy problem: bad/missing path or query param, or a pydantic
    /// validation failure on the request body.
    Validation(String),
    /// The handler itself raised - a plain 500, same as an unhandled FastAPI exception.
    Handler(String),
}

/// Builds the kwargs for a route handler from the matched path params, the
/// query string, and the raw body, then invokes it. This is the "exactly like
/// FastAPI" dependency-injection step: each declared parameter is bound by
/// name according to how it was classified at route-registration time.
fn dispatch_route(
    py: Python<'_>,
    handler: &Py<PyAny>,
    params: &[ParamMeta],
    method: &str,
    full_path: &str,
    path_params: &HashMap<String, String>,
    query_params: &HashMap<String, String>,
    body: &str,
    convert_value_fn: &PyObject,
) -> Result<PyObject, DispatchError> {
    if params.is_empty() {
        return handler.call0(py).map_err(|e| DispatchError::Handler(e.to_string()));
    }

    let kwargs = PyDict::new_bound(py);

    for p in params {
        match p.kind {
            ParamKind::Request => {
                let req = PyRequest {
                    method: method.to_string(),
                    path: full_path.to_string(),
                    path_params: path_params.clone(),
                    query_params: query_params.clone(),
                    body: body.to_string(),
                };
                let obj = Py::new(py, req).map_err(|e| DispatchError::Handler(e.to_string()))?;
                kwargs.set_item(&p.name, obj)
                    .map_err(|e| DispatchError::Handler(e.to_string()))?;
            }
            ParamKind::Path => {
                let raw = path_params.get(&p.name).cloned().unwrap_or_default();
                let converted = convert_value_fn.bind(py)
                    .call1((raw, p.effective_type.bind(py)))
                    .map_err(|_| DispatchError::Validation(format!(
                        "Invalid value for path parameter '{}'", p.name
                    )))?;
                kwargs.set_item(&p.name, converted)
                    .map_err(|e| DispatchError::Handler(e.to_string()))?;
            }
            ParamKind::Query => {
                if let Some(raw) = query_params.get(&p.name) {
                    let converted = convert_value_fn.bind(py)
                        .call1((raw, p.effective_type.bind(py)))
                        .map_err(|_| DispatchError::Validation(format!(
                            "Invalid value for query parameter '{}'", p.name
                        )))?;
                    kwargs.set_item(&p.name, converted)
                        .map_err(|e| DispatchError::Handler(e.to_string()))?;
                } else if p.required {
                    return Err(DispatchError::Validation(format!(
                        "Missing required query parameter '{}'", p.name
                    )));
                } else {
                    kwargs.set_item(&p.name, p.default.clone_ref(py))
                        .map_err(|e| DispatchError::Handler(e.to_string()))?;
                }
            }
            ParamKind::Body => {
                let model = p.pydantic_model.as_ref().expect("Body param must carry a pydantic model");
                let py_val = if body.is_empty() {
                    PyDict::new_bound(py).into_any()
                } else {
                    py.import_bound("json")
                        .and_then(|j| j.call_method1("loads", (body,)))
                        .map_err(|e| DispatchError::Validation(format!("Invalid JSON body: {e}")))?
                        .into_any()
                };
                let instance = model.bind(py).call_method1("model_validate", (py_val,))
                    .map_err(|e| DispatchError::Validation(e.to_string()))?;
                kwargs.set_item(&p.name, instance)
                    .map_err(|e| DispatchError::Handler(e.to_string()))?;
            }
        }
    }

    handler.bind(py).call((), Some(&kwargs))
        .map(|v| v.into())
        .map_err(|e| DispatchError::Handler(e.to_string()))
}

async fn handle(
    req: HyperRequest<Body>,
    routes: Routes,
    serializer: Arc<PyObject>,
    schedule_coro: Arc<PyObject>,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    convert_value_fn: Arc<PyObject>,
) -> Result<HyperResponse<Body>, Infallible> {
    let start_time = Instant::now();

    // Extract variables early so req can be safely consumed by into_body()
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_params = parse_query(req.uri().query());

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

    let (status, resp_body, content_type) = if method == "GET" && path == "/docs" {
        (200, swagger_html(), "text/html")
    } else if method == "GET" && path == "/openapi.json" {
        let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
        (200, spec, "application/json")
    } else if method == "POST" && path == "/mcp" {
        let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
        let req_method = req_json["method"].as_str().unwrap_or("");
        let msg_id = req_json["id"].clone();
        let params = req_json.get("params").unwrap_or(&json!({})).clone();

        let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
        let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

        let result = if req_method == "initialize" {
            ok(json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
                "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}
            }))
        } else if req_method == "notifications/initialized" || req_method == "initialized" {
            String::new()
        } else if req_method == "ping" {
            ok(json!({}))
        } else if req_method == "tools/list" {
            let guard = tools.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|t| {
                json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })
            }).collect();
            ok(json!({"tools": items}))
        } else if req_method == "resources/list" {
            let guard = resources.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|r| {
                json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })
            }).collect();
            ok(json!({"resources": items}))
        } else if req_method == "prompts/list" {
            let guard = prompts.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|p| {
                json!({ "name": p.name, "description": p.description, "arguments": [] })
            }).collect();
            ok(json!({"prompts": items}))
        } else if req_method == "tools/call" {
            let name = params["name"].as_str().unwrap_or("");
            let args_json = params["arguments"].clone();

            let tool_opt = Python::with_gil(|py| {
                let guard = tools.lock().unwrap();
                guard.iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async))
            });

            if let Some((handler, is_async)) = tool_opt {
                let exec_res = Python::with_gil(|py| -> PyResult<PyObject> {
                    let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                    if let Ok(dict) = kwargs.downcast::<PyDict>() {
                        handler.bind(py).call((), Some(dict)).map(|v| v.into())
                    } else {
                        handler.bind(py).call0().map(|v| v.into())
                    }
                });
                execute_python_handler(exec_res, is_async, &serializer, &schedule_coro).await
                    .map(|s| ok(json!({"content": [{"type": "text", "text": s}], "isError": false})))
                    .unwrap_or_else(|e| ok(json!({"content": [{"type": "text", "text": e}], "isError": true})))
            } else { err(-32602, &format!("Unknown tool: {}", name)) }
        } else if req_method == "resources/read" {
            let uri = params["uri"].as_str().unwrap_or("");
            let res_opt = Python::with_gil(|py| {
                let guard = resources.lock().unwrap();
                guard.iter().find(|r| r.uri == uri).map(|r| (r.handler.clone_ref(py), r.is_async, r.mime_type.clone()))
            });
            if let Some((handler, is_async, mime)) = res_opt {
                let exec_res = Python::with_gil(|py| handler.call0(py));
                execute_python_handler(exec_res, is_async, &serializer, &schedule_coro).await
                    .map(|s| ok(json!({"contents": [{"uri": uri, "mimeType": mime, "text": s}]})))
                    .unwrap_or_else(|e| err(-32603, &e))
            } else { err(-32602, &format!("Unknown resource: {}", uri)) }
        } else if req_method == "prompts/get" {
            let name = params["name"].as_str().unwrap_or("");
            let args_json = params["arguments"].clone();
            let pro_opt = Python::with_gil(|py| {
                let guard = prompts.lock().unwrap();
                guard.iter().find(|p| p.name == name).map(|p| (p.handler.clone_ref(py), p.is_async))
            });
            if let Some((handler, is_async)) = pro_opt {
                let exec_res = Python::with_gil(|py| -> PyResult<PyObject> {
                    let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                    if let Ok(dict) = kwargs.downcast::<PyDict>() {
                        handler.bind(py).call((), Some(dict)).map(|v| v.into())
                    } else {
                        handler.bind(py).call0().map(|v| v.into())
                    }
                });
                execute_python_handler(exec_res, is_async, &serializer, &schedule_coro).await
                    .map(|s| ok(json!({"messages": [{"role": "user", "content": {"type": "text", "text": s}}]})))
                    .unwrap_or_else(|e| err(-32603, &e))
            } else { err(-32602, &format!("Unknown prompt: {}", name)) }
        } else {
            err(-32601, &format!("Method not found: {}", req_method))
        };

        if result.is_empty() { (202, result, "application/json") } else { (200, result, "application/json") }

    } else {
        let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };

        match matched {
            Some((idx, path_params)) => {
                // FIXED: We now safely map over the Vec to clone each ParamMeta respecting the GIL
                let (handler, is_async, params) = Python::with_gil(|py| {
                    let guard = routes.lock().unwrap();
                    let entry = &guard[idx];
                    let cloned_params = entry.params.iter().map(|p| p.clone_ref(py)).collect::<Vec<_>>();
                    (entry.handler.clone_ref(py), entry.is_async, cloned_params)
                });

                let dispatch_res = Python::with_gil(|py| {
                    dispatch_route(
                        py, &handler, &params, &method, &path,
                        &path_params, &query_params, &body, &convert_value_fn,
                    )
                });

                match dispatch_res {
                    Ok(exec_obj) => {
                        match execute_python_handler(Ok(exec_obj), is_async, &serializer, &schedule_coro).await {
                            Ok(s) => (200, s, "application/json"),
                            Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e.replace('"', "'")), "application/json"),
                        }
                    }
                    Err(DispatchError::Validation(msg)) => {
                        (422, json!({"detail": msg}).to_string(), "application/json")
                    }
                    Err(DispatchError::Handler(msg)) => {
                        (500, format!(r#"{{"error":"{}"}}"#, msg.replace('"', "'")), "application/json")
                    }
                }
            }
            None => (404, r#"{"error":"not found"}"#.to_string(), "application/json"),
        }
    };

    println!("[INFO] {} {} - {} ({}ms)", method, path, status, start_time.elapsed().as_millis());
    Ok(HyperResponse::builder().status(status).header("Content-Type", content_type).body(Body::from(resp_body)).unwrap())
}

async fn execute_python_handler(
    exec_res: PyResult<PyObject>,
    is_async: bool,
    serializer: &PyObject,
    schedule_coro: &PyObject
) -> Result<String, String> {
    if is_async {
        let (tx, rx) = oneshot::channel();
        let spawn_res = Python::with_gil(|py| -> PyResult<()> {
            let coro = exec_res?;
            let cb = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) })?;
            schedule_coro.bind(py).call1((coro, cb))?;
            Ok(())
        });
        if let Err(e) = spawn_res { return Err(e.to_string()); }

        let result_obj = rx.await.map_err(|_| "Asyncio channel dropped".to_string())??;
        Python::with_gil(|py| -> PyResult<String> {
            if result_obj.is_none(py) { Ok(String::new()) }
            else if let Ok(s) = result_obj.downcast_bound::<PyString>(py) { Ok(s.to_string()) }
            else { serializer.bind(py).call1((result_obj,))?.extract() }
        }).map_err(|e| e.to_string())
    } else {
        let py_obj = exec_res.map_err(|e| e.to_string())?;
        Python::with_gil(|py| -> PyResult<String> {
            if py_obj.is_none(py) { Ok(String::new()) }
            else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { Ok(s.to_string()) }
            else { serializer.bind(py).call1((py_obj,))?.extract() }
        }).map_err(|e| e.to_string())
    }
}

// ---------- Decorators ----------

#[pyclass]
struct RouteDecorator {
    routes: Routes,
    method: String,
    path: String,
    effective_type_fn: PyObject,
    type_name_fn: PyObject,
}

#[allow(non_local_definitions)]
#[pymethods]
impl RouteDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?;
        let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let sig = inspect.call_method1("signature", (func.bind(py),))?;
        let sig_params = sig.getattr("parameters")?;
        let empty = inspect.getattr("Parameter")?.getattr("empty")?;
        let req_type = py.get_type_bound::<PyRequest>();

        let path_param_names: HashSet<String> = extract_path_params(&self.path).into_iter().collect();

        let mut params = Vec::new();
        let mut body_schema_json: Option<String> = None;

        if let Ok(values) = sig_params.call_method0("values") {
            if let Ok(iter) = values.iter() {
                for p_res in iter {
                    let p = p_res?;
                    let name: String = p.getattr("name")?.extract()?;
                    let annotation = p.getattr("annotation")?;
                    let default_obj = p.getattr("default")?;
                    let has_default = !default_obj.is(&empty);

                    // 1. Body: a pydantic model annotation.
                    if annotation.hasattr("model_json_schema").unwrap_or(false) {
                        if body_schema_json.is_none() {
                            if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
                                if let Ok(s) = py.import_bound("json")?
                                    .call_method1("dumps", (schema_dict,))?
                                    .extract::<String>()
                                {
                                    body_schema_json = Some(s);
                                }
                            }
                        }
                        params.push(ParamMeta {
                            name,
                            kind: ParamKind::Body,
                            effective_type: py.None(),
                            type_name: "object".to_string(),
                            required: true,
                            default: py.None(),
                            pydantic_model: Some(annotation.clone().unbind()),
                        });
                        continue;
                    }

                    // 2. Request: explicit `PyRequest` annotation, or conventional naming.
                    let is_request_type = annotation.is(req_type.as_any());
                    if is_request_type || name == "request" || name == "req" {
                        params.push(ParamMeta {
                            name,
                            kind: ParamKind::Request,
                            effective_type: py.None(),
                            type_name: "request".to_string(),
                            required: false,
                            default: py.None(),
                            pydantic_model: None,
                        });
                        continue;
                    }

                    // 3. Path or Query, based on whether the name appears in the route pattern.
                    let eff_type = self.effective_type_fn.bind(py).call1((&annotation,))?;
                    let type_name: String = self.type_name_fn.bind(py).call1((&eff_type,))?.extract()?;
                    let kind = if path_param_names.contains(&name) { ParamKind::Path } else { ParamKind::Query };
                    let default: Py<PyAny> = if has_default { default_obj.clone().unbind() } else { py.None() };

                    params.push(ParamMeta {
                        name,
                        kind,
                        effective_type: eff_type.unbind(),
                        type_name,
                        required: !has_default,
                        default,
                        pydantic_model: None,
                    });
                }
            }
        }

        self.routes.lock().unwrap().push(RouteEntry {
            method: self.method.clone(),
            original_path: self.path.clone(),
            segments: parse_pattern(&self.path),
            handler: func.clone_ref(py),
            is_async,
            params,
            body_schema_json,
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
fn rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<PyRequest>()?;
    m.add_function(wrap_pyfunction!(compute, m)?)?;
    Ok(())
}