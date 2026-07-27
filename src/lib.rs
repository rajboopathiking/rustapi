// use pyo3::prelude::*;
// use pyo3::wrap_pyfunction;
// use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex};
// use std::thread;
// use std::time::{Duration, Instant};

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server};
// use notify::{RecursiveMode, Watcher};
// use serde_json::json;
// use tokio::sync::{oneshot, Semaphore};

// const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// #[derive(Clone)]
// enum Segment { Literal(String), Param(String) }

// struct DependencyMeta {
//     name: String, func: Py<PyAny>, is_async: bool, is_generator: bool, use_cache: bool, id: isize,
// }

// impl Clone for DependencyMeta {
//     fn clone(&self) -> Self {
//         Python::with_gil(|py| DependencyMeta {
//             name: self.name.clone(), func: self.func.clone_ref(py), is_async: self.is_async,
//             is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
//         })
//     }
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     pydantic_param_name: Option<String>,
//     request_schema_json: Option<String>,
//     request_param_name: Option<String>,
//     background_task_param_name: Option<String>,
//     dependencies: Vec<DependencyMeta>,
// }

// type Routes = Arc<Mutex<Vec<RouteEntry>>>;

// struct ToolEntry { name: String, description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
// struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
// struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

// type Tools = Arc<Mutex<Vec<ToolEntry>>>;
// type Resources = Arc<Mutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<Mutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path).into_iter().map(|s| {
//         if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
//         else { Segment::Literal(s.to_string()) }
//     }).collect()
// }

// fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() { continue; }
//         let mut params = HashMap::new(); let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => { if l != val { ok = false; break; } },
//                 Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
//             }
//         }
//         if ok { return Some((idx, params)); }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// fn build_openapi_spec(routes: &[RouteEntry]) -> serde_json::Value {
//     let mut paths = serde_json::Map::new();
//     for route in routes {
//         let path_item = paths.entry(route.original_path.clone()).or_insert_with(|| json!({}));
//         if let Some(path_obj) = path_item.as_object_mut() {
//             let method_key = route.method.to_lowercase();
//             path_obj.insert(
//                 method_key,
//                 json!({
//                     "responses": {
//                         "200": { "description": "Successful Response" }
//                     }
//                 }),
//             );
//         }
//     }

//     json!({
//         "openapi": "3.0.0",
//         "info": {
//             "title": "RustAPI",
//             "version": "1.0.0"
//         },
//         "paths": paths
//     })
// }

// // MULTIPART PARSER STRUCT
// #[pyclass(name = "UploadFile")]
// #[derive(Clone)]
// struct PyUploadFile {
//     #[pyo3(get)] filename: String,
//     #[pyo3(get)] content_type: String,
//     file_data: Vec<u8>,
// }

// #[pymethods]
// impl PyUploadFile {
//     fn read(&self, py: Python<'_>) -> PyObject {
//         PyBytes::new_bound(py, &self.file_data).into()
//     }
// }

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
//     #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
//     #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
//     #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
// }

// #[pyclass(name = "Response")]
// struct PyResponse {
//     #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
// }

// impl Clone for PyResponse {
//     fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
// }

// #[pymethods]
// impl PyResponse {
//     #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
//     fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
// }

// #[pyclass]
// struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

// #[pymethods]
// impl CoroCallback {
//     #[pyo3(signature = (result, error))]
//     fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
//         if let Some(tx) = self.tx.lock().unwrap().take() {
//             if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
//         }
//     }
// }

// #[pyclass]
// struct Engine {
//     routes: Routes, serializer: PyObject, _tools: Tools, _resources: Resources, _prompts: Prompts,
//     _schema_fn: PyObject, schedule_coro_fn: PyObject,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let python_code = r#"
// import asyncio, json, threading
// _engine_loop = asyncio.new_event_loop()
// def _start_engine_loop():
//     asyncio.set_event_loop(_engine_loop)
//     _engine_loop.run_forever()
// threading.Thread(target=_start_engine_loop, daemon=True).start()
// def _schedule_coro(coro, callback):
//     def done_cb(fut):
//         try: callback(fut.result(), None)
//         except Exception as e: callback(None, e)
//     fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
//     fut.add_done_callback(done_cb)
// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
// "#;
//         let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
//         Ok(Engine {
//             routes: Arc::new(Mutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
//             schedule_coro_fn: module.getattr("_schedule_coro")?.into(), _schema_fn: py.None(),
//             _tools: Arc::new(Mutex::new(Vec::new())), _resources: Arc::new(Mutex::new(Vec::new())), _prompts: Arc::new(Mutex::new(Vec::new())),
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path } }
//     fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path } }
    
//     #[pyo3(signature = (router, prefix="".to_string()))]
//     fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
//         let routes_obj = router.getattr(py, "routes")?;
//         let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
//         for (method, path, func) in routes {
//             let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
//             match method.as_str() {
//                 "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
//                 "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
//                 "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
//             }
//         }
//         Ok(())
//     }

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
//         let safe_workers = if workers < 1 { 1 } else { workers };

//         if (reload || safe_workers > 1) && !is_worker {
//             let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let spawn_children = || {
//                     let mut nc = Vec::new();
//                     for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
//                     nc
//                 };
//                 let mut children = spawn_children();
//                 let (tx, rx) = std::sync::mpsc::channel();
//                 let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
//                 loop {
//                     if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
//                         if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
//                             for mut child in children { let _ = child.kill(); let _ = child.wait(); }
//                             children = spawn_children(); continue;
//                         }
//                     }} else { thread::sleep(Duration::from_millis(250)); }
//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
//                 }
//             });
//             if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
//             return Ok(());
//         }

//         let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
//         let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
//         socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
//         socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
//         let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
//         let routes = self.routes.clone(); let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
//         let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_| {
//                     let (r, s, sc, sem) = (routes.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
//                     async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), sem.clone()))) }
//                 });
//                 let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
//                 if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() { return None; }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });
//         let _ = server_handle.join();
//         if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
//     }
// }

// async fn handle(
//     mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, gil_sem: Arc<Semaphore>,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
//     let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
//     for (k, v) in req.headers() {
//         let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
//         if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
//         headers_map.insert(key_str, val_str);
//     }
    
//     // NATIVE RUST MULTIPART PARSING
//     let mut form_map = HashMap::new();
//     let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
//     let mut body_bytes = Vec::new();
//     let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
//     if let Ok(boundary) = multer::parse_boundary(content_type) {
//         let mut multipart = multer::Multipart::new(req.into_body(), boundary);
//         while let Ok(Some(field)) = multipart.next_field().await {
//             let name = field.name().unwrap_or("").to_string();
//             let file_name = field.file_name().map(|s| s.to_string());
//             let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
//             if let Some(fn_str) = file_name {
//                 let data = field.bytes().await.unwrap_or_default().to_vec();
//                 files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
//             } else {
//                 let text = field.text().await.unwrap_or_default();
//                 form_map.insert(name, text);
//             }
//         }
//     } else {
//         let mut body_stream = req.into_body();
//         while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
//             let chunk = chunk_res.unwrap_or_default();
//             if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
//             body_bytes.extend_from_slice(&chunk);
//         }
//     }
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, resp_headers) = if method == "GET" && path == "/openapi.json" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//         let guard = routes.lock().unwrap();
//         let spec = build_openapi_spec(&guard);
//         let body = serde_json::to_string(&spec).unwrap_or_else(|_| r#"{"openapi":"3.0.0"}"#.to_string());
//         (200, body, h)
//     } else if method == "GET" && path == "/docs" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
//         let html = r#"<!DOCTYPE html>
// <html lang="en">
//   <head>
//     <meta charset="utf-8" />
//     <meta name="viewport" content="width=device-width, initial-scale=1" />
//     <title>Swagger UI</title>
//     <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
//   </head>
//   <body>
//     <div id="swagger-ui"></div>
//     <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
//     <script>
//       window.onload = () => {
//         SwaggerUIBundle({
//           url: '/openapi.json',
//           dom_id: '#swagger-ui',
//           deepLinking: true,
//           presets: [SwaggerUIBundle.presets.apis],
//         });
//       };
//     </script>
//   </body>
// </html>"#;
//         (200, html.to_string(), h)
//     } else {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps) = Python::with_gil(|py| {
//                     let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                     (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone())
//                 });

//                 let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
//                 let form_c = form_map.clone(); let files_c = files_map.clone();
                
//                 let mut dependency_error: Option<String> = None;
//                 let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

//                 for dep in deps {
//                     if dep.use_cache && cache.contains_key(&dep.id) {
//                         let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
//                         resolved_args.insert(dep.name.clone(), cached_val); continue;
//                     }
//                     let dep_result_res: Result<PyObject, String> = if dep.is_async {
//                         let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
//                         match coro_res {
//                             Ok(coro) => {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
//                                 match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
//                             }
//                             Err(e) => Err(e),
//                         }
//                     } else {
//                         let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
//                         tokio::task::spawn_blocking(move || {
//                             let _permit = sem_clone.try_acquire().ok();
//                             Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
//                         }).await.unwrap_or_else(|_| Err("Panic".to_string()))
//                     };

//                     match dep_result_res {
//                         Ok(dep_obj) => {
//                             let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
//                             match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
//                         }
//                         Err(e) => { dependency_error = Some(e); break; }
//                     }
//                 }

//                 if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

//                 let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
//                     Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
//                 } else { None };

//                 let sem_clone = gil_sem.clone();
//                 let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
//                 let bg_param_name_clone = background_task_param_name.clone();

//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     let _permit = sem_clone.try_acquire().ok();
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = pyo3::types::PyDict::new_bound(py);
//                         for (k, v) in &path_params_c { kwargs.set_item(k, v)?; }
//                         for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
//                         if let Some(req_name) = request_param_name {
//                             let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
//                             kwargs.set_item(req_name, req_obj)?;
//                         }
//                         if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
//                         handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

//                 let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
//                 if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

//                 if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

//                 if let Some(bg_obj) = bg_tasks_obj {
//                     let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
//                     if let Some(tasks) = tasks_list {
//                         let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
//                         tokio::spawn(async move {
//                             for (func, args, kw) in tasks {
//                                 let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
//                                 if is_async {
//                                     let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
//                                     if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
//                                 } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
//                             }
//                         });
//                     }
//                 }
//                 (r_status, r_body, r_headers)
//             }
//             None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
//         }
//     };

//     let mut builder = HyperResponse::builder().status(status);
//     for (k, v) in resp_headers { builder = builder.header(&k, &v); }
//     Ok(builder.body(Body::from(resp_body)).unwrap())
// }

// async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
//     let py_result: PyResult<PyObject> = if is_async {
//         match exec_res {
//             Ok(coro) => {
//                 let (tx, rx) = oneshot::channel();
//                 let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
//                 if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio channel dropped"))), } }
//             }
//             Err(e) => Err(e),
//         }
//     } else { exec_res };

//     Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
//         match py_result {
//             Ok(py_obj) => {
//                 if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
//                     let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
//                     let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
//                     return (status, body_str, headers);
//                 }
//                 let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
//                 (200, body_str, HashMap::new())
//             }
//             Err(err) => {
//                 (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
//             }
//         }
//     })
// }

// #[pyclass]
// struct RouteDecorator { routes: Routes, method: String, path: String }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
//         let mut request_param_name = None; let mut background_task_param_name = None; let mut dependencies = Vec::new(); 
//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         let param_name: String = p.getattr("name")?.extract()?;
//                         if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
//                         if let Ok(annotation) = p.getattr("annotation") { if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } } }
//                         if let Ok(default_val) = p.getattr("default") {
//                             let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
//                             if is_depends {
//                                 let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
//                                 if !dep_func.is_none() {
//                                     let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let dep_id = dep_func.as_ptr() as isize;
//                                     dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         self.routes.lock().unwrap().push(RouteEntry { method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path), handler: func.clone_ref(py), is_async, pydantic_model: None, pydantic_param_name: None, request_schema_json: None, request_param_name, background_task_param_name, dependencies });
//         Ok(func)
//     }
// }

// #[pymodule]
// fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?; 
//     m.add_class::<PyRequest>()?; 
//     m.add_class::<PyResponse>()?; 
//     m.add_class::<PyUploadFile>()?;
//     Ok(())
// }



// use pyo3::prelude::*;
// use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex};
// use std::thread;
// use std::time::Duration;

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
// use notify::{RecursiveMode, Watcher};
// use serde_json::json;
// use tokio::sync::{oneshot, Semaphore};
// use futures_util::{StreamExt, SinkExt};

// const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// #[derive(Clone)]
// enum Segment { Literal(String), Param(String) }

// struct DependencyMeta {
//     name: String, func: Py<PyAny>, is_async: bool, is_generator: bool, use_cache: bool, id: isize,
// }

// impl Clone for DependencyMeta {
//     fn clone(&self) -> Self {
//         Python::with_gil(|py| DependencyMeta {
//             name: self.name.clone(), func: self.func.clone_ref(py), is_async: self.is_async,
//             is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
//         })
//     }
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     pydantic_param_name: Option<String>,
//     _request_schema_json: Option<String>,
//     request_param_name: Option<String>,
//     background_task_param_name: Option<String>,
//     websocket_param_name: Option<String>,
//     is_websocket: bool,
//     dependencies: Vec<DependencyMeta>,
// }

// type Routes = Arc<Mutex<Vec<RouteEntry>>>;

// struct ToolEntry { name: String, description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
// struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
// struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

// type Tools = Arc<Mutex<Vec<ToolEntry>>>;
// type Resources = Arc<Mutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<Mutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path).into_iter().map(|s| {
//         if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
//         else { Segment::Literal(s.to_string()) }
//     }).collect()
// }

// fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() { continue; }
//         let mut params = HashMap::new(); let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => { if l != val { ok = false; break; } },
//                 Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
//             }
//         }
//         if ok { return Some((idx, params)); }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// fn generate_openapi(routes: &[RouteEntry]) -> String {
//     let mut paths = serde_json::Map::new();
//     for r in routes {
//         if r.is_websocket { continue; }
//         let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });
//         if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
//             if r.original_path.contains("upload") {
//                 method_obj["requestBody"] = json!({
//                     "required": true,
//                     "content": {
//                         "multipart/form-data": {
//                             "schema": {
//                                 "type": "object",
//                                 "properties": {
//                                     "document": { "type": "string", "format": "binary", "description": "File to upload" },
//                                     "description": { "type": "string", "description": "Form description field" }
//                                 }
//                             }
//                         }
//                     }
//                 });
//             } else {
//                 method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } } });
//             }
//         }
//         let method_lower = r.method.to_lowercase();
//         if let Some(path_item) = paths.get_mut(&r.original_path) {
//             path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
//         } else {
//             paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
//         }
//     }
//     serde_json::to_string(&json!({ "openapi": "3.0.0", "info": { "title": "RustAPI", "version": "0.1.0" }, "paths": paths })).unwrap()
// }

// fn swagger_html() -> String {
//     r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>Swagger UI - RustAPI</title><link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script><script>window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };</script></body></html>"#.to_string()
// }

// #[pyclass(name = "WebSocket")]
// struct PyWebSocket {
//     stream: Arc<Mutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
// }

// #[pymethods]
// impl PyWebSocket {
//     fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 while let Some(msg) = lock.next().await {
//                     if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
//                         return Ok(text);
//                     }
//                 }
//                 Err(pyo3::exceptions::PyConnectionAbortedError::new_err("Connection closed"))
//             })
//         })
//     }

//     fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 lock.send(tokio_tungstenite::tungstenite::Message::Text(text)).await
//                     .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
//                 Ok(())
//             })
//         })
//     }
// }

// #[pyclass(name = "UploadFile")]
// #[derive(Clone)]
// struct PyUploadFile {
//     #[pyo3(get)] filename: String,
//     #[pyo3(get)] content_type: String,
//     file_data: Vec<u8>,
// }

// #[pymethods]
// impl PyUploadFile {
//     fn read(&self, py: Python<'_>) -> PyObject { PyBytes::new_bound(py, &self.file_data).into() }
// }

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
//     #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
//     #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
//     #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
// }

// #[pyclass(name = "Response")]
// struct PyResponse {
//     #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
// }

// impl Clone for PyResponse {
//     fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
// }

// #[pymethods]
// impl PyResponse {
//     #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
//     fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
// }

// #[pyclass]
// struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

// #[pymethods]
// impl CoroCallback {
//     #[pyo3(signature = (result, error))]
//     fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
//         if let Some(tx) = self.tx.lock().unwrap().take() {
//             if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
//         }
//     }
// }

// #[pyclass]
// struct Engine {
//     routes: Routes, serializer: PyObject, tools: Tools, resources: Resources, prompts: Prompts,
//     schema_fn: PyObject, schedule_coro_fn: PyObject,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let python_code = r#"
// import asyncio, inspect, json, threading
// _engine_loop = asyncio.new_event_loop()
// def _start_engine_loop():
//     asyncio.set_event_loop(_engine_loop)
//     _engine_loop.run_forever()
// threading.Thread(target=_start_engine_loop, daemon=True).start()
// def _schedule_coro(coro, callback):
//     def done_cb(fut):
//         try: callback(fut.result(), None)
//         except Exception as e: callback(None, e)
//     fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
//     fut.add_done_callback(done_cb)
// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
// def _schema_from_signature(func):
//     sig = inspect.signature(func)
//     props = {name: {"type": "string"} for name in sig.parameters}
//     return {"type": "object", "properties": props}
// "#;
//         let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
//         Ok(Engine {
//             routes: Arc::new(Mutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
//             schedule_coro_fn: module.getattr("_schedule_coro")?.into(), schema_fn: module.getattr("_schema_from_signature")?.into(),
//             tools: Arc::new(Mutex::new(Vec::new())), resources: Arc::new(Mutex::new(Vec::new())), prompts: Arc::new(Mutex::new(Vec::new())),
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: false } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path, is_ws: false } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path, is_ws: false } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
//     fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path, is_ws: false } }
//     fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: true } }
    
//     #[pyo3(signature = (router, prefix="".to_string()))]
//     fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
//         let routes_obj = router.getattr(py, "routes")?;
//         let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
//         for (method, path, func) in routes {
//             let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
//             match method.as_str() {
//                 "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
//                 "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
//                 "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
//             }
//         }
//         Ok(())
//     }

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

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
//         let safe_workers = if workers < 1 { 1 } else { workers };

//         if (reload || safe_workers > 1) && !is_worker {
//             let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let spawn_children = || {
//                     let mut nc = Vec::new();
//                     for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
//                     nc
//                 };
//                 let mut children = spawn_children();
//                 let (tx, rx) = std::sync::mpsc::channel();
//                 let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
//                 loop {
//                     if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
//                         if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
//                             for mut child in children { let _ = child.kill(); let _ = child.wait(); }
//                             children = spawn_children(); continue;
//                         }
//                     }} else { thread::sleep(Duration::from_millis(250)); }
//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
//                 }
//             });
//             if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
//             return Ok(());
//         }

//         let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
//         let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
//         socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
//         socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
//         let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
//         let routes = self.routes.clone(); let tools = self.tools.clone(); let resources = self.resources.clone(); let prompts = self.prompts.clone();
//         let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
//         let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_| {
//                     let (r, t, res, p, s, sc, sem) = (routes.clone(), tools.clone(), resources.clone(), prompts.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
//                     async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone()))) }
//                 });
//                 let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
//                 if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() { return None; }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });
//         let _ = server_handle.join();
//         if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
//     }
// }

// async fn handle(
//     mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, tools: Tools, resources: Resources, prompts: Prompts, gil_sem: Arc<Semaphore>,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
//     let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
//     for (k, v) in req.headers() {
//         let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
//         if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
//         headers_map.insert(key_str, val_str);
//     }
    
//     // WEBSOCKET UPGRADE CHECK
//     let is_ws_upgrade = req.headers().get(hyper::header::UPGRADE)
//         .and_then(|v| v.to_str().ok())
//         .map(|s| s.eq_ignore_ascii_case("websocket"))
//         .unwrap_or(false);

//     if is_ws_upgrade {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
//         if let Some((idx, _)) = matched {
//             let (handler, ws_param_name) = Python::with_gil(|py| {
//                 let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                 (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
//             });

//             if let Some(ws_name) = ws_param_name {
//                 let res = hyper::upgrade::on(&req).await;
//                 if let Ok(upgraded) = res {
//                     let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
//                         upgraded,
//                         tokio_tungstenite::tungstenite::protocol::Role::Server,
//                         None,
//                     ).await;

//                     let ws_obj = Arc::new(tokio::sync::Mutex::new(ws_stream));
//                     let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());
//                     let schedule_coro_ws = schedule_coro.clone();

//                     tokio::spawn(async move {
//                         let coro = Python::with_gil(|py| {
//                             let kwargs = pyo3::types::PyDict::new_bound(py);
//                             let _ = kwargs.set_item(ws_name, ws_py_obj.bind(py));
//                             handler.bind(py).call((), Some(&kwargs)).map(|b| b.unbind()).ok()
//                         });
//                         if let Some(c) = coro {
//                             let (tx, rx) = oneshot::channel();
//                             Python::with_gil(|py| {
//                                 if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) {
//                                     let _ = schedule_coro_ws.bind(py).call1((c, cb));
//                                 }
//                             });
//                             let _ = rx.await;
//                         }
//                     });

//                     return Ok(HyperResponse::builder().status(StatusCode::SWITCHING_PROTOCOLS).body(Body::empty()).unwrap());
//                 }
//             }
//         }
//     }

//     let mut form_map = HashMap::new();
//     let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
//     let mut body_bytes = Vec::new();
//     let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
//     if let Ok(boundary) = multer::parse_boundary(content_type) {
//         let mut multipart = multer::Multipart::new(req.into_body(), boundary);
//         while let Ok(Some(field)) = multipart.next_field().await {
//             let name = field.name().unwrap_or("").to_string();
//             let file_name = field.file_name().map(|s| s.to_string());
//             let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
//             if let Some(fn_str) = file_name {
//                 let data = field.bytes().await.unwrap_or_default().to_vec();
//                 files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
//             } else {
//                 let text = field.text().await.unwrap_or_default();
//                 form_map.insert(name, text);
//             }
//         }
//     } else {
//         let mut body_stream = req.into_body();
//         while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
//             let chunk = chunk_res.unwrap_or_default();
//             if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
//             body_bytes.extend_from_slice(&chunk);
//         }
//     }
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string()); (200, swagger_html(), h)
//     } else if method == "GET" && path == "/openapi.json" {
//         let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (200, spec, h)
//     } else if method == "POST" && path == "/mcp" {
//         let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
//         let req_method = req_json["method"].as_str().unwrap_or("").to_string();
//         let has_id = req_json.get("id").is_some();
//         let msg_id = req_json["id"].clone();
//         let params = req_json.get("params").unwrap_or(&json!({})).clone();
//         let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
//         let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

//         let result = if !has_id { String::new() }
//         else if req_method == "initialize" { ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}})) }
//         else if req_method == "tools/list" {
//             let guard = tools.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })).collect();
//             ok(json!({"tools": items}))
//         } else if req_method == "resources/list" {
//             let guard = resources.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
//             ok(json!({"resources": items}))
//         } else if req_method == "prompts/list" {
//             let guard = prompts.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
//             ok(json!({"prompts": items}))
//         } else if req_method == "tools/call" {
//             let name = params["name"].as_str().unwrap_or("").to_string();
//             let args_json = params["arguments"].clone();
//             let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async)));
//             if let Some((handler, is_async)) = tool_opt {
//                 let _permit = gil_sem.acquire().await.ok();
//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
//                         if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));
//                 let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
//                 if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
//             } else { err(-32602, &format!("Unknown tool: {}", name)) }
//         } else { err(-32601, &format!("Method not found: {}", req_method)) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//         (200, result, h)
//     } else {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps) = Python::with_gil(|py| {
//                     let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                     (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone())
//                 });

//                 let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
//                 let form_c = form_map.clone(); let files_c = files_map.clone();
                
//                 let mut dependency_error: Option<String> = None;
//                 let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

//                 for dep in deps {
//                     if dep.use_cache && cache.contains_key(&dep.id) {
//                         let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
//                         resolved_args.insert(dep.name.clone(), cached_val); continue;
//                     }
//                     let dep_result_res: Result<PyObject, String> = if dep.is_async {
//                         let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
//                         match coro_res {
//                             Ok(coro) => {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
//                                 match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
//                             }
//                             Err(e) => Err(e),
//                         }
//                     } else {
//                         let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
//                         tokio::task::spawn_blocking(move || {
//                             let _permit = sem_clone.try_acquire().ok();
//                             Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
//                         }).await.unwrap_or_else(|_| Err("Panic".to_string()))
//                     };

//                     match dep_result_res {
//                         Ok(dep_obj) => {
//                             let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
//                             match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
//                         }
//                         Err(e) => { dependency_error = Some(e); break; }
//                     }
//                 }

//                 if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

//                 let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
//                     Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
//                 } else { None };

//                 let sem_clone = gil_sem.clone();
//                 let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
//                 let bg_param_name_clone = background_task_param_name.clone();

//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     let _permit = sem_clone.try_acquire().ok();
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = pyo3::types::PyDict::new_bound(py);
//                         for (k, v) in &path_params_c { kwargs.set_item(k, v)?; }
//                         for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
//                         if let Some(req_name) = request_param_name {
//                             let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
//                             kwargs.set_item(req_name, req_obj)?;
//                         }
//                         if let Some(ref model) = pydantic_model {
//                             let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() } else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
//                             let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
//                             if let Some(model_name) = pydantic_param_name { kwargs.set_item(model_name, instance)?; }
//                         }
//                         if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
//                         handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

//                 let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
//                 if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

//                 if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

//                 if let Some(bg_obj) = bg_tasks_obj {
//                     let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
//                     if let Some(tasks) = tasks_list {
//                         let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
//                         tokio::spawn(async move {
//                             for (func, args, kw) in tasks {
//                                 let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
//                                 if is_async {
//                                     let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
//                                     if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
//                                 } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
//                             }
//                         });
//                     }
//                 }
//                 (r_status, r_body, r_headers)
//             }
//             None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
//         }
//     };

//     let mut builder = HyperResponse::builder().status(status);
//     for (k, v) in resp_headers { builder = builder.header(&k, &v); }
//     Ok(builder.body(Body::from(resp_body)).unwrap())
// }

// async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
//     let py_result: PyResult<PyObject> = if is_async {
//         match exec_res {
//             Ok(coro) => {
//                 let (tx, rx) = oneshot::channel();
//                 let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: Mutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
//                 if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))), } }
//             }
//             Err(e) => Err(e),
//         }
//     } else { exec_res };

//     Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
//         match py_result {
//             Ok(py_obj) => {
//                 if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
//                     let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
//                     let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
//                     return (status, body_str, headers);
//                 }
//                 let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
//                 (200, body_str, HashMap::new())
//             }
//             Err(err) => {
//                 (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
//             }
//         }
//     })
// }

// #[pyclass]
// struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
//         let mut pydantic_model = None; let mut pydantic_param_name = None; let mut request_schema_json = None;
//         let mut request_param_name = None; let mut background_task_param_name = None; let mut websocket_param_name = None; let mut dependencies = Vec::new(); 
//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         let param_name: String = p.getattr("name")?.extract()?;
//                         if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
//                         if self.is_ws { websocket_param_name = Some(param_name.clone()); continue; }
//                         if let Ok(annotation) = p.getattr("annotation") {
//                             if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } }
//                             if annotation.hasattr("model_json_schema").unwrap_or(false) {
//                                 pydantic_model = Some(annotation.clone().into()); pydantic_param_name = Some(param_name.clone());
//                                 if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
//                                     if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) { if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); } }
//                                 }
//                                 continue; 
//                             }
//                         }
//                         if let Ok(default_val) = p.getattr("default") {
//                             let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
//                             if is_depends {
//                                 let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
//                                 if !dep_func.is_none() {
//                                     let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let dep_id = dep_func.as_ptr() as isize;
//                                     dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         self.routes.lock().unwrap().push(RouteEntry {
//             method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path),
//             handler: func.clone_ref(py), is_async, pydantic_model, pydantic_param_name,
//             _request_schema_json: request_schema_json, request_param_name, background_task_param_name,
//             websocket_param_name, is_websocket: self.is_ws, dependencies,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ToolDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
//         let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
//         self.tools.lock().unwrap().push(ToolEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             schema_json: serde_json::from_str(&schema_str).unwrap(), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ResourceDecorator { resources: Resources, uri: String, mime_type: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ResourceDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.resources.lock().unwrap().push(ResourceEntry {
//             uri: self.uri.clone(), description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct PromptDecorator { prompts: Prompts, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl PromptDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.prompts.lock().unwrap().push(PromptEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: self.description.clone().unwrap_or_else(|| py.import_bound("inspect").unwrap().call_method1("getdoc", (func.bind(py),)).unwrap().extract().unwrap_or_default()),
//             handler: func.clone_ref(py), is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pymodule]
// fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?; 
//     m.add_class::<PyRequest>()?; 
//     m.add_class::<PyResponse>()?; 
//     m.add_class::<PyUploadFile>()?;
//     m.add_class::<PyWebSocket>()?;
//     Ok(())
// }






// use pyo3::prelude::*;
// use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex as StdMutex};
// use std::thread;
// use std::time::Duration;

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
// use notify::{RecursiveMode, Watcher};
// use serde_json::json;
// use tokio::sync::{oneshot, Semaphore, Mutex as TokioMutex};
// use futures_util::{StreamExt, SinkExt};

// const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// #[derive(Clone)]
// enum Segment { Literal(String), Param(String) }

// struct DependencyMeta {
//     name: String, func: Py<PyAny>, is_async: bool, is_generator: bool, use_cache: bool, id: isize,
// }

// impl Clone for DependencyMeta {
//     fn clone(&self) -> Self {
//         Python::with_gil(|py| DependencyMeta {
//             name: self.name.clone(), func: self.func.clone_ref(py), is_async: self.is_async,
//             is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
//         })
//     }
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     pydantic_param_name: Option<String>,
//     _request_schema_json: Option<String>,
//     request_param_name: Option<String>,
//     background_task_param_name: Option<String>,
//     websocket_param_name: Option<String>,
//     is_websocket: bool,
//     dependencies: Vec<DependencyMeta>,
// }

// type Routes = Arc<StdMutex<Vec<RouteEntry>>>;

// struct ToolEntry { name: String, description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
// struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
// struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

// type Tools = Arc<StdMutex<Vec<ToolEntry>>>;
// type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<StdMutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path).into_iter().map(|s| {
//         if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
//         else { Segment::Literal(s.to_string()) }
//     }).collect()
// }

// fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() { continue; }
//         let mut params = HashMap::new(); let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => { if l != val { ok = false; break; } },
//                 Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
//             }
//         }
//         if ok { return Some((idx, params)); }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// fn generate_openapi(routes: &[RouteEntry]) -> String {
//     let mut paths = serde_json::Map::new();
//     for r in routes {
//         if r.is_websocket { continue; }
//         let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });
//         if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
//             if r.original_path.contains("upload") {
//                 method_obj["requestBody"] = json!({
//                     "required": true,
//                     "content": {
//                         "multipart/form-data": {
//                             "schema": {
//                                 "type": "object",
//                                 "properties": {
//                                     "document": { "type": "string", "format": "binary", "description": "File to upload" },
//                                     "description": { "type": "string", "description": "Form description field" }
//                                 }
//                             }
//                         }
//                     }
//                 });
//             } else {
//                 method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } } });
//             }
//         }
//         let method_lower = r.method.to_lowercase();
//         if let Some(path_item) = paths.get_mut(&r.original_path) {
//             path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
//         } else {
//             paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
//         }
//     }
//     serde_json::to_string(&json!({ "openapi": "3.0.0", "info": { "title": "RustAPI", "version": "0.1.0" }, "paths": paths })).unwrap()
// }

// fn swagger_html() -> String {
//     r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>Swagger UI - RustAPI</title><link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script><script>window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };</script></body></html>"#.to_string()
// }

// #[pyclass(name = "WebSocket")]
// struct PyWebSocket {
//     stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
// }

// #[pymethods]
// impl PyWebSocket {
//     fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 while let Some(msg) = lock.next().await {
//                     if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
//                         return Ok(text.to_string());
//                     }
//                 }
//                 Err(pyo3::exceptions::PyConnectionAbortedError::new_err("Connection closed"))
//             })
//         })
//     }

//     fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 lock.send(tokio_tungstenite::tungstenite::Message::Text(text.into())).await
//                     .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
//                 Ok(())
//             })
//         })
//     }
// }

// #[pyclass(name = "UploadFile")]
// #[derive(Clone)]
// struct PyUploadFile {
//     #[pyo3(get)] filename: String,
//     #[pyo3(get)] content_type: String,
//     file_data: Vec<u8>,
// }

// #[pymethods]
// impl PyUploadFile {
//     fn read(&self, py: Python<'_>) -> PyObject { PyBytes::new_bound(py, &self.file_data).into() }
// }

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
//     #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
//     #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
//     #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
// }

// #[pyclass(name = "Response")]
// struct PyResponse {
//     #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
// }

// impl Clone for PyResponse {
//     fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
// }

// #[pymethods]
// impl PyResponse {
//     #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
//     fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
// }

// #[pyclass]
// struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

// #[pymethods]
// impl CoroCallback {
//     #[pyo3(signature = (result, error))]
//     fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
//         if let Some(tx) = self.tx.lock().unwrap().take() {
//             if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
//         }
//     }
// }

// #[pyclass]
// struct Engine {
//     routes: Routes, serializer: PyObject, tools: Tools, resources: Resources, prompts: Prompts,
//     schema_fn: PyObject, schedule_coro_fn: PyObject,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let python_code = r#"
// import asyncio, inspect, json, threading
// _engine_loop = asyncio.new_event_loop()
// def _start_engine_loop():
//     asyncio.set_event_loop(_engine_loop)
//     _engine_loop.run_forever()
// threading.Thread(target=_start_engine_loop, daemon=True).start()
// def _schedule_coro(coro, callback):
//     def done_cb(fut):
//         try: callback(fut.result(), None)
//         except Exception as e: callback(None, e)
//     fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
//     fut.add_done_callback(done_cb)
// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
// def _schema_from_signature(func):
//     sig = inspect.signature(func)
//     props = {name: {"type": "string"} for name in sig.parameters}
//     return {"type": "object", "properties": props}
// "#;
//         let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
//         Ok(Engine {
//             routes: Arc::new(StdMutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
//             schedule_coro_fn: module.getattr("_schedule_coro")?.into(), schema_fn: module.getattr("_schema_from_signature")?.into(),
//             tools: Arc::new(StdMutex::new(Vec::new())), resources: Arc::new(StdMutex::new(Vec::new())), prompts: Arc::new(StdMutex::new(Vec::new())),
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: false } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path, is_ws: false } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path, is_ws: false } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
//     fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path, is_ws: false } }
//     fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: true } }
    
//     #[pyo3(signature = (router, prefix="".to_string()))]
//     fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
//         let routes_obj = router.getattr(py, "routes")?;
//         let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
//         for (method, path, func) in routes {
//             let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
//             match method.as_str() {
//                 "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
//                 "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
//                 "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
//             }
//         }
//         Ok(())
//     }

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

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
//         let safe_workers = if workers < 1 { 1 } else { workers };

//         if (reload || safe_workers > 1) && !is_worker {
//             let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let spawn_children = || {
//                     let mut nc = Vec::new();
//                     for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
//                     nc
//                 };
//                 let mut children = spawn_children();
//                 let (tx, rx) = std::sync::mpsc::channel();
//                 let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
//                 loop {
//                     if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
//                         if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
//                             for mut child in children { let _ = child.kill(); let _ = child.wait(); }
//                             children = spawn_children(); continue;
//                         }
//                     }} else { thread::sleep(Duration::from_millis(250)); }
//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
//                 }
//             });
//             if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
//             return Ok(());
//         }

//         let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
//         let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
//         socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
//         socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
//         let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
//         let routes = self.routes.clone(); let tools = self.tools.clone(); let resources = self.resources.clone(); let prompts = self.prompts.clone();
//         let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
//         let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_| {
//                     let (r, t, res, p, s, sc, sem) = (routes.clone(), tools.clone(), resources.clone(), prompts.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
//                     async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone()))) }
//                 });
//                 let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
//                 if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() { return None; }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });
//         let _ = server_handle.join();
//         if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
//     }
// }

// async fn handle(
//     mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, tools: Tools, resources: Resources, prompts: Prompts, gil_sem: Arc<Semaphore>,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
//     let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
//     for (k, v) in req.headers() {
//         let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
//         if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
//         headers_map.insert(key_str, val_str);
//     }
    
//     // WEBSOCKET UPGRADE CHECK (Must verify match first before consuming `req`)
//     let is_ws_upgrade = req.headers().get(hyper::header::UPGRADE)
//         .and_then(|v| v.to_str().ok())
//         .map(|s| s.eq_ignore_ascii_case("websocket"))
//         .unwrap_or(false);

//     if is_ws_upgrade {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
//         if let Some((idx, _)) = matched {
//             let (handler, ws_param_name) = Python::with_gil(|py| {
//                 let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                 (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
//             });

//             if let Some(ws_name) = ws_param_name {
//                 match tungstenite::handshake::server::create_response(&req) {
//                     Ok(tungstenite_resp) => {
//                         let schedule_coro_ws = schedule_coro.clone();
//                         tokio::spawn(async move {
//                             let res = hyper::upgrade::on(req).await;
//                             if let Ok(upgraded) = res {
//                                 let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
//                                     upgraded,
//                                     tokio_tungstenite::tungstenite::protocol::Role::Server,
//                                     None,
//                                 ).await;

//                                 let ws_obj = Arc::new(TokioMutex::new(ws_stream));
//                                 let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());

//                                 let coro = Python::with_gil(|py| {
//                                     let kwargs = pyo3::types::PyDict::new_bound(py);
//                                     let _ = kwargs.set_item(ws_name, ws_py_obj.bind(py));
//                                     handler.bind(py).call((), Some(&kwargs)).map(|b| b.unbind()).ok()
//                                 });
//                                 if let Some(c) = coro {
//                                     let (tx, rx) = oneshot::channel();
//                                     Python::with_gil(|py| {
//                                         if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
//                                             let _ = schedule_coro_ws.bind(py).call1((c, cb));
//                                         }
//                                     });
//                                     let _ = rx.await;
//                                 }
//                             }
//                         });

//                         let (parts, _) = tungstenite_resp.into_parts();
//                         return Ok(HyperResponse::from_parts(parts, Body::empty()));
//                     }
//                     Err(_) => {}
//                 }
//             }
//         }
//     }

//     let mut form_map = HashMap::new();
//     let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
//     let mut body_bytes = Vec::new();
//     let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
//     if let Ok(boundary) = multer::parse_boundary(content_type) {
//         let mut multipart = multer::Multipart::new(req.into_body(), boundary);
//         while let Ok(Some(field)) = multipart.next_field().await {
//             let name = field.name().unwrap_or("").to_string();
//             let file_name = field.file_name().map(|s| s.to_string());
//             let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
//             if let Some(fn_str) = file_name {
//                 let data = field.bytes().await.unwrap_or_default().to_vec();
//                 files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
//             } else {
//                 let text = field.text().await.unwrap_or_default();
//                 form_map.insert(name, text);
//             }
//         }
//     } else {
//         let mut body_stream = req.into_body();
//         while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
//             let chunk = chunk_res.unwrap_or_default();
//             if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
//             body_bytes.extend_from_slice(&chunk);
//         }
//     }
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string()); (200, swagger_html(), h)
//     } else if method == "GET" && path == "/openapi.json" {
//         let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (200, spec, h)
//     } else if method == "POST" && path == "/mcp" {
//         let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
//         let req_method = req_json["method"].as_str().unwrap_or("").to_string();
//         let has_id = req_json.get("id").is_some();
//         let msg_id = req_json["id"].clone();
//         let params = req_json.get("params").unwrap_or(&json!({})).clone();
//         let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
//         let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

//         let result = if !has_id { String::new() }
//         else if req_method == "initialize" { ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}})) }
//         else if req_method == "tools/list" {
//             let guard = tools.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })).collect();
//             ok(json!({"tools": items}))
//         } else if req_method == "resources/list" {
//             let guard = resources.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
//             ok(json!({"resources": items}))
//         } else if req_method == "prompts/list" {
//             let guard = prompts.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
//             ok(json!({"prompts": items}))
//         } else if req_method == "tools/call" {
//             let name = params["name"].as_str().unwrap_or("").to_string();
//             let args_json = params["arguments"].clone();
//             let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async)));
//             if let Some((handler, is_async)) = tool_opt {
//                 let _permit = gil_sem.acquire().await.ok();
//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
//                         if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));
//                 let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
//                 if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
//             } else { err(-32602, &format!("Unknown tool: {}", name)) }
//         } else { err(-32601, &format!("Method not found: {}", req_method)) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//         (200, result, h)
//     } else {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps) = Python::with_gil(|py| {
//                     let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                     (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone())
//                 });

//                 let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
//                 let form_c = form_map.clone(); let files_c = files_map.clone();
                
//                 let mut dependency_error: Option<String> = None;
//                 let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

//                 for dep in deps {
//                     if dep.use_cache && cache.contains_key(&dep.id) {
//                         let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
//                         resolved_args.insert(dep.name.clone(), cached_val); continue;
//                     }
//                     let dep_result_res: Result<PyObject, String> = if dep.is_async {
//                         let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
//                         match coro_res {
//                             Ok(coro) => {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
//                                 match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
//                             }
//                             Err(e) => Err(e),
//                         }
//                     } else {
//                         let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
//                         tokio::task::spawn_blocking(move || {
//                             let _permit = sem_clone.try_acquire().ok();
//                             Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
//                         }).await.unwrap_or_else(|_| Err("Panic".to_string()))
//                     };

//                     match dep_result_res {
//                         Ok(dep_obj) => {
//                             let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
//                             match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
//                         }
//                         Err(e) => { dependency_error = Some(e); break; }
//                     }
//                 }

//                 if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

//                 let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
//                     Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
//                 } else { None };

//                 let sem_clone = gil_sem.clone();
//                 let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
//                 let bg_param_name_clone = background_task_param_name.clone();

//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     let _permit = sem_clone.try_acquire().ok();
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = pyo3::types::PyDict::new_bound(py);
//                         for (k, v) in &path_params_c { kwargs.set_item(k, v)?; }
//                         for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
//                         if let Some(req_name) = request_param_name {
//                             let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
//                             kwargs.set_item(req_name, req_obj)?;
//                         }
//                         if let Some(ref model) = pydantic_model {
//                             let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() } else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
//                             let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
//                             if let Some(model_name) = pydantic_param_name { kwargs.set_item(model_name, instance)?; }
//                         }
//                         if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
//                         handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

//                 let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
//                 if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

//                 if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

//                 if let Some(bg_obj) = bg_tasks_obj {
//                     let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
//                     if let Some(tasks) = tasks_list {
//                         let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
//                         tokio::spawn(async move {
//                             for (func, args, kw) in tasks {
//                                 let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
//                                 if is_async {
//                                     let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
//                                     if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
//                                 } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
//                             }
//                         });
//                     }
//                 }
//                 (r_status, r_body, r_headers)
//             }
//             None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
//         }
//     };

//     let mut builder = HyperResponse::builder().status(status);
//     for (k, v) in resp_headers { builder = builder.header(&k, &v); }
//     Ok(builder.body(Body::from(resp_body)).unwrap())
// }

// async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
//     let py_result: PyResult<PyObject> = if is_async {
//         match exec_res {
//             Ok(coro) => {
//                 let (tx, rx) = oneshot::channel();
//                 let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
//                 if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))), } }
//             }
//             Err(e) => Err(e),
//         }
//     } else { exec_res };

//     Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
//         match py_result {
//             Ok(py_obj) => {
//                 if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
//                     let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
//                     let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
//                     return (status, body_str, headers);
//                 }
//                 let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
//                 (200, body_str, HashMap::new())
//             }
//             Err(err) => {
//                 (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
//             }
//         }
//     })
// }

// #[pyclass]
// struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
//         let mut pydantic_model = None; let mut pydantic_param_name = None; let mut request_schema_json = None;
//         let mut request_param_name = None; let mut background_task_param_name = None; let mut websocket_param_name = None; let mut dependencies = Vec::new(); 
//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         let param_name: String = p.getattr("name")?.extract()?;
//                         if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
//                         if self.is_ws { websocket_param_name = Some(param_name.clone()); continue; }
//                         if let Ok(annotation) = p.getattr("annotation") {
//                             if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } }
//                             if annotation.hasattr("model_json_schema").unwrap_or(false) {
//                                 pydantic_model = Some(annotation.clone().into()); pydantic_param_name = Some(param_name.clone());
//                                 if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
//                                     if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) { if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); } }
//                                 }
//                                 continue; 
//                             }
//                         }
//                         if let Ok(default_val) = p.getattr("default") {
//                             let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
//                             if is_depends {
//                                 let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
//                                 if !dep_func.is_none() {
//                                     let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let dep_id = dep_func.as_ptr() as isize;
//                                     dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         self.routes.lock().unwrap().push(RouteEntry {
//             method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path),
//             handler: func.clone_ref(py), is_async, pydantic_model, pydantic_param_name,
//             _request_schema_json: request_schema_json, request_param_name, background_task_param_name,
//             websocket_param_name, is_websocket: self.is_ws, dependencies,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ToolDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
//         let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
//         self.tools.lock().unwrap().push(ToolEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             schema_json: serde_json::from_str(&schema_str).unwrap(), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ResourceDecorator { resources: Resources, uri: String, mime_type: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ResourceDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.resources.lock().unwrap().push(ResourceEntry {
//             uri: self.uri.clone(), description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct PromptDecorator { prompts: Prompts, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl PromptDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.prompts.lock().unwrap().push(PromptEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: self.description.clone().unwrap_or_else(|| py.import_bound("inspect").unwrap().call_method1("getdoc", (func.bind(py),)).unwrap().extract().unwrap_or_default()),
//             handler: func.clone_ref(py), is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pymodule]
// fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?; 
//     m.add_class::<PyRequest>()?; 
//     m.add_class::<PyResponse>()?; 
//     m.add_class::<PyUploadFile>()?;
//     m.add_class::<PyWebSocket>()?;
//     Ok(())
// }






// use pyo3::prelude::*;
// use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex as StdMutex};
// use std::thread;
// use std::time::Duration;

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
// use notify::{RecursiveMode, Watcher};
// use serde_json::json;
// use tokio::sync::{oneshot, Semaphore, Mutex as TokioMutex};
// use futures_util::{StreamExt, SinkExt};
// use sha1::{Sha1, Digest};
// use base64::{Engine as _, engine::general_purpose};

// const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// #[derive(Clone)]
// enum Segment { Literal(String), Param(String) }

// struct DependencyMeta {
//     name: String, func: Py<PyAny>, is_async: bool, is_generator: bool, use_cache: bool, id: isize,
// }

// impl Clone for DependencyMeta {
//     fn clone(&self) -> Self {
//         Python::with_gil(|py| DependencyMeta {
//             name: self.name.clone(), func: self.func.clone_ref(py), is_async: self.is_async,
//             is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
//         })
//     }
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     pydantic_param_name: Option<String>,
//     _request_schema_json: Option<String>,
//     request_param_name: Option<String>,
//     background_task_param_name: Option<String>,
//     websocket_param_name: Option<String>,
//     is_websocket: bool,
//     dependencies: Vec<DependencyMeta>,
// }

// type Routes = Arc<StdMutex<Vec<RouteEntry>>>;

// struct ToolEntry { name: String, description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
// struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
// struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

// type Tools = Arc<StdMutex<Vec<ToolEntry>>>;
// type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<StdMutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path).into_iter().map(|s| {
//         if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
//         else { Segment::Literal(s.to_string()) }
//     }).collect()
// }

// fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() { continue; }
//         let mut params = HashMap::new(); let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => { if l != val { ok = false; break; } },
//                 Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
//             }
//         }
//         if ok { return Some((idx, params)); }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// fn compute_websocket_accept(key: &str) -> String {
//     let mut sha1 = Sha1::new();
//     sha1.update(key.as_bytes());
//     sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
//     general_purpose::STANDARD.encode(sha1.finalize())
// }

// fn generate_openapi(routes: &[RouteEntry]) -> String {
//     let mut paths = serde_json::Map::new();
//     for r in routes {
//         if r.is_websocket { continue; }
//         let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });
//         if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
//             if r.original_path.contains("upload") {
//                 method_obj["requestBody"] = json!({
//                     "required": true,
//                     "content": {
//                         "multipart/form-data": {
//                             "schema": {
//                                 "type": "object",
//                                 "properties": {
//                                     "document": { "type": "string", "format": "binary", "description": "File to upload" },
//                                     "description": { "type": "string", "description": "Form description field" }
//                                 }
//                             }
//                         }
//                     }
//                 });
//             } else {
//                 method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } } });
//             }
//         }
//         let method_lower = r.method.to_lowercase();
//         if let Some(path_item) = paths.get_mut(&r.original_path) {
//             path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
//         } else {
//             paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
//         }
//     }
//     serde_json::to_string(&json!({ "openapi": "3.0.0", "info": { "title": "RustAPI", "version": "0.1.0" }, "paths": paths })).unwrap()
// }

// fn swagger_html() -> String {
//     r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>Swagger UI - RustAPI</title><link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script><script>window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };</script></body></html>"#.to_string()
// }

// #[pyclass(name = "WebSocket")]
// struct PyWebSocket {
//     stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
// }

// #[pymethods]
// impl PyWebSocket {
//     fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 while let Some(msg) = lock.next().await {
//                     if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
//                         return Ok(text.to_string());
//                     }
//                 }
//                 Err(pyo3::exceptions::PyConnectionAbortedError::new_err("Connection closed"))
//             })
//         })
//     }

//     fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 lock.send(tokio_tungstenite::tungstenite::Message::Text(text.into())).await
//                     .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
//                 Ok(())
//             })
//         })
//     }
// }

// #[pyclass(name = "UploadFile")]
// #[derive(Clone)]
// struct PyUploadFile {
//     #[pyo3(get)] filename: String,
//     #[pyo3(get)] content_type: String,
//     file_data: Vec<u8>,
// }

// #[pymethods]
// impl PyUploadFile {
//     fn read(&self, py: Python<'_>) -> PyObject { PyBytes::new_bound(py, &self.file_data).into() }
// }

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
//     #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
//     #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
//     #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
// }

// #[pyclass(name = "Response")]
// struct PyResponse {
//     #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
// }

// impl Clone for PyResponse {
//     fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
// }

// #[pymethods]
// impl PyResponse {
//     #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
//     fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
// }

// #[pyclass]
// struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

// #[pymethods]
// impl CoroCallback {
//     #[pyo3(signature = (result, error))]
//     fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
//         if let Some(tx) = self.tx.lock().unwrap().take() {
//             if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
//         }
//     }
// }

// #[pyclass]
// struct Engine {
//     routes: Routes, serializer: PyObject, tools: Tools, resources: Resources, prompts: Prompts,
//     schema_fn: PyObject, schedule_coro_fn: PyObject,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let python_code = r#"
// import asyncio, inspect, json, threading
// _engine_loop = asyncio.new_event_loop()
// def _start_engine_loop():
//     asyncio.set_event_loop(_engine_loop)
//     _engine_loop.run_forever()
// threading.Thread(target=_start_engine_loop, daemon=True).start()
// def _schedule_coro(coro, callback):
//     def done_cb(fut):
//         try: callback(fut.result(), None)
//         except Exception as e: callback(None, e)
//     fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
//     fut.add_done_callback(done_cb)
// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
// def _schema_from_signature(func):
//     sig = inspect.signature(func)
//     props = {name: {"type": "string"} for name in sig.parameters}
//     return {"type": "object", "properties": props}
// "#;
//         let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
//         Ok(Engine {
//             routes: Arc::new(StdMutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
//             schedule_coro_fn: module.getattr("_schedule_coro")?.into(), schema_fn: module.getattr("_schema_from_signature")?.into(),
//             tools: Arc::new(StdMutex::new(Vec::new())), resources: Arc::new(StdMutex::new(Vec::new())), prompts: Arc::new(StdMutex::new(Vec::new())),
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: false } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path, is_ws: false } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path, is_ws: false } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
//     fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path, is_ws: false } }
//     fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: true } }
    
//     #[pyo3(signature = (router, prefix="".to_string()))]
//     fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
//         let routes_obj = router.getattr(py, "routes")?;
//         let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
//         for (method, path, func) in routes {
//             let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
//             match method.as_str() {
//                 "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
//                 "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
//                 "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
//             }
//         }
//         Ok(())
//     }

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

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
//         let safe_workers = if workers < 1 { 1 } else { workers };

//         if (reload || safe_workers > 1) && !is_worker {
//             let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let spawn_children = || {
//                     let mut nc = Vec::new();
//                     for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
//                     nc
//                 };
//                 let mut children = spawn_children();
//                 let (tx, rx) = std::sync::mpsc::channel();
//                 let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
//                 loop {
//                     if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
//                         if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
//                             for mut child in children { let _ = child.kill(); let _ = child.wait(); }
//                             children = spawn_children(); continue;
//                         }
//                     }} else { thread::sleep(Duration::from_millis(250)); }
//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
//                 }
//             });
//             if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
//             return Ok(());
//         }

//         let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
//         let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
//         socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
//         socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
//         let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
//         let routes = self.routes.clone(); let tools = self.tools.clone(); let resources = self.resources.clone(); let prompts = self.prompts.clone();
//         let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
//         let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_| {
//                     let (r, t, res, p, s, sc, sem) = (routes.clone(), tools.clone(), resources.clone(), prompts.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
//                     async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone()))) }
//                 });
//                 let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
//                 if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() { return None; }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });
//         let _ = server_handle.join();
//         if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
//     }
// }

// async fn handle(
//     mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, tools: Tools, resources: Resources, prompts: Prompts, gil_sem: Arc<Semaphore>,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
//     let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
//     for (k, v) in req.headers() {
//         let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
//         if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
//         headers_map.insert(key_str, val_str);
//     }
    
//     // WEBSOCKET UPGRADE CHECK (Manual Handshake)
//     let is_ws_upgrade = req.headers().get(hyper::header::UPGRADE)
//         .and_then(|v| v.to_str().ok())
//         .map(|s| s.eq_ignore_ascii_case("websocket"))
//         .unwrap_or(false);

//     if is_ws_upgrade {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
//         if let Some((idx, _)) = matched {
//             let (handler, ws_param_name) = Python::with_gil(|py| {
//                 let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                 (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
//             });

//             if let Some(ws_name) = ws_param_name {
//                 if let Some(ws_key) = req.headers().get("sec-websocket-key").and_then(|v| v.to_str().ok()) {
//                     let accept_key = compute_websocket_accept(ws_key);
//                     let schedule_coro_ws = schedule_coro.clone();

//                     tokio::spawn(async move {
//                         let res = hyper::upgrade::on(&mut req).await;
//                         if let Ok(upgraded) = res {
//                             let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
//                                 upgraded,
//                                 tokio_tungstenite::tungstenite::protocol::Role::Server,
//                                 None,
//                             ).await;

//                             let ws_obj = Arc::new(TokioMutex::new(ws_stream));
//                             let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());

//                             let coro = Python::with_gil(|py| {
//                                 let kwargs = pyo3::types::PyDict::new_bound(py);
//                                 let _ = kwargs.set_item(ws_name, ws_py_obj.bind(py));
//                                 handler.bind(py).call((), Some(&kwargs)).map(|b| b.unbind()).ok()
//                             });
//                             if let Some(c) = coro {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| {
//                                     if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
//                                         let _ = schedule_coro_ws.bind(py).call1((c, cb));
//                                     }
//                                 });
//                                 let _ = rx.await;
//                             }
//                         }
//                     });

//                     return Ok(HyperResponse::builder()
//                         .status(StatusCode::SWITCHING_PROTOCOLS)
//                         .header(hyper::header::UPGRADE, "websocket")
//                         .header(hyper::header::CONNECTION, "upgrade")
//                         .header("sec-websocket-accept", accept_key)
//                         .body(Body::empty())
//                         .unwrap());
//                 }
//             }
//         }
//     }

//     let mut form_map = HashMap::new();
//     let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
//     let mut body_bytes = Vec::new();
//     let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
//     if let Ok(boundary) = multer::parse_boundary(content_type) {
//         let mut multipart = multer::Multipart::new(req.into_body(), boundary);
//         while let Ok(Some(field)) = multipart.next_field().await {
//             let name = field.name().unwrap_or("").to_string();
//             let file_name = field.file_name().map(|s| s.to_string());
//             let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
//             if let Some(fn_str) = file_name {
//                 let data = field.bytes().await.unwrap_or_default().to_vec();
//                 files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
//             } else {
//                 let text = field.text().await.unwrap_or_default();
//                 form_map.insert(name, text);
//             }
//         }
//     } else {
//         let mut body_stream = req.into_body();
//         while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
//             let chunk = chunk_res.unwrap_or_default();
//             if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
//             body_bytes.extend_from_slice(&chunk);
//         }
//     }
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string()); (200, swagger_html(), h)
//     } else if method == "GET" && path == "/openapi.json" {
//         let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (200, spec, h)
//     } else if method == "POST" && path == "/mcp" {
//         let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
//         let req_method = req_json["method"].as_str().unwrap_or("").to_string();
//         let has_id = req_json.get("id").is_some();
//         let msg_id = req_json["id"].clone();
//         let params = req_json.get("params").unwrap_or(&json!({})).clone();
//         let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
//         let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

//         let result = if !has_id { String::new() }
//         else if req_method == "initialize" { ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}})) }
//         else if req_method == "tools/list" {
//             let guard = tools.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })).collect();
//             ok(json!({"tools": items}))
//         } else if req_method == "resources/list" {
//             let guard = resources.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
//             ok(json!({"resources": items}))
//         } else if req_method == "prompts/list" {
//             let guard = prompts.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
//             ok(json!({"prompts": items}))
//         } else if req_method == "tools/call" {
//             let name = params["name"].as_str().unwrap_or("").to_string();
//             let args_json = params["arguments"].clone();
//             let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async)));
//             if let Some((handler, is_async)) = tool_opt {
//                 let _permit = gil_sem.acquire().await.ok();
//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
//                         if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));
//                 let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
//                 if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
//             } else { err(-32602, &format!("Unknown tool: {}", name)) }
//         } else { err(-32601, &format!("Method not found: {}", req_method)) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//         (200, result, h)
//     } else {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps) = Python::with_gil(|py| {
//                     let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                     (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone())
//                 });

//                 let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
//                 let form_c = form_map.clone(); let files_c = files_map.clone();
                
//                 let mut dependency_error: Option<String> = None;
//                 let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

//                 for dep in deps {
//                     if dep.use_cache && cache.contains_key(&dep.id) {
//                         let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
//                         resolved_args.insert(dep.name.clone(), cached_val); continue;
//                     }
//                     let dep_result_res: Result<PyObject, String> = if dep.is_async {
//                         let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
//                         match coro_res {
//                             Ok(coro) => {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
//                                 match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
//                             }
//                             Err(e) => Err(e),
//                         }
//                     } else {
//                         let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
//                         tokio::task::spawn_blocking(move || {
//                             let _permit = sem_clone.try_acquire().ok();
//                             Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
//                         }).await.unwrap_or_else(|_| Err("Panic".to_string()))
//                     };

//                     match dep_result_res {
//                         Ok(dep_obj) => {
//                             let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
//                             match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
//                         }
//                         Err(e) => { dependency_error = Some(e); break; }
//                     }
//                 }

//                 if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

//                 let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
//                     Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
//                 } else { None };

//                 let sem_clone = gil_sem.clone();
//                 let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
//                 let bg_param_name_clone = background_task_param_name.clone();

//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     let _permit = sem_clone.try_acquire().ok();
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = pyo3::types::PyDict::new_bound(py);
//                         for (k, v) in &path_params_c { kwargs.set_item(k, v)?; }
//                         for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
//                         if let Some(req_name) = request_param_name {
//                             let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
//                             kwargs.set_item(req_name, req_obj)?;
//                         }
//                         if let Some(ref model) = pydantic_model {
//                             let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() } else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
//                             let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
//                             if let Some(model_name) = pydantic_param_name { kwargs.set_item(model_name, instance)?; }
//                         }
//                         if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
//                         handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

//                 let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
//                 if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

//                 if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

//                 if let Some(bg_obj) = bg_tasks_obj {
//                     let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
//                     if let Some(tasks) = tasks_list {
//                         let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
//                         tokio::spawn(async move {
//                             for (func, args, kw) in tasks {
//                                 let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
//                                 if is_async {
//                                     let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
//                                     if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
//                                 } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
//                             }
//                         });
//                     }
//                 }
//                 (r_status, r_body, r_headers)
//             }
//             None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
//         }
//     };

//     let mut builder = HyperResponse::builder().status(status);
//     for (k, v) in resp_headers { builder = builder.header(&k, &v); }
//     Ok(builder.body(Body::from(resp_body)).unwrap())
// }

// async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
//     let py_result: PyResult<PyObject> = if is_async {
//         match exec_res {
//             Ok(coro) => {
//                 let (tx, rx) = oneshot::channel();
//                 let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
//                 if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))), } }
//             }
//             Err(e) => Err(e),
//         }
//     } else { exec_res };

//     Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
//         match py_result {
//             Ok(py_obj) => {
//                 if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
//                     let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
//                     let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
//                     return (status, body_str, headers);
//                 }
//                 let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
//                 (200, body_str, HashMap::new())
//             }
//             Err(err) => {
//                 (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
//             }
//         }
//     })
// }

// #[pyclass]
// struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
//         let mut pydantic_model = None; let mut pydantic_param_name = None; let mut request_schema_json = None;
//         let mut request_param_name = None; let mut background_task_param_name = None; let mut websocket_param_name = None; let mut dependencies = Vec::new(); 
//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         let param_name: String = p.getattr("name")?.extract()?;
//                         if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
//                         if self.is_ws { websocket_param_name = Some(param_name.clone()); continue; }
//                         if let Ok(annotation) = p.getattr("annotation") {
//                             if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } }
//                             if annotation.hasattr("model_json_schema").unwrap_or(false) {
//                                 pydantic_model = Some(annotation.clone().into()); pydantic_param_name = Some(param_name.clone());
//                                 if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
//                                     if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) { if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); } }
//                                 }
//                                 continue; 
//                             }
//                         }
//                         if let Ok(default_val) = p.getattr("default") {
//                             let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
//                             if is_depends {
//                                 let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
//                                 if !dep_func.is_none() {
//                                     let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let dep_id = dep_func.as_ptr() as isize;
//                                     dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         self.routes.lock().unwrap().push(RouteEntry {
//             method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path),
//             handler: func.clone_ref(py), is_async, pydantic_model, pydantic_param_name,
//             _request_schema_json: request_schema_json, request_param_name, background_task_param_name,
//             websocket_param_name, is_websocket: self.is_ws, dependencies,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ToolDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
//         let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
//         self.tools.lock().unwrap().push(ToolEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             schema_json: serde_json::from_str(&schema_str).unwrap(), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ResourceDecorator { resources: Resources, uri: String, mime_type: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ResourceDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.resources.lock().unwrap().push(ResourceEntry {
//             uri: self.uri.clone(), description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct PromptDecorator { prompts: Prompts, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl PromptDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.prompts.lock().unwrap().push(PromptEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: self.description.clone().unwrap_or_else(|| py.import_bound("inspect").unwrap().call_method1("getdoc", (func.bind(py),)).unwrap().extract().unwrap_or_default()),
//             handler: func.clone_ref(py), is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pymodule]
// fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?; 
//     m.add_class::<PyRequest>()?; 
//     m.add_class::<PyResponse>()?; 
//     m.add_class::<PyUploadFile>()?;
//     m.add_class::<PyWebSocket>()?;
//     Ok(())
// }


// use pyo3::prelude::*;
// use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
// use std::collections::HashMap;
// use std::convert::Infallible;
// use std::net::SocketAddr;
// use std::path::Path;
// use std::process::Command;
// use std::sync::{mpsc, Arc, Mutex as StdMutex};
// use std::thread;
// use std::time::Duration;

// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
// use notify::{RecursiveMode, Watcher};
// use serde_json::json;
// use tokio::sync::{oneshot, Semaphore, Mutex as TokioMutex};
// use futures_util::{StreamExt, SinkExt};
// use sha1::{Sha1, Digest};
// use base64::{Engine as _, engine::general_purpose};

// const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

// #[derive(Clone)]
// enum Segment { Literal(String), Param(String) }

// struct DependencyMeta {
//     name: String, func: Py<PyAny>, is_async: bool, is_generator: bool, use_cache: bool, id: isize,
// }

// impl Clone for DependencyMeta {
//     fn clone(&self) -> Self {
//         Python::with_gil(|py| DependencyMeta {
//             name: self.name.clone(), func: self.func.clone_ref(py), is_async: self.is_async,
//             is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
//         })
//     }
// }

// struct RouteEntry {
//     method: String,
//     original_path: String,
//     segments: Vec<Segment>,
//     handler: Py<PyAny>,
//     is_async: bool,
//     pydantic_model: Option<Py<PyAny>>,
//     pydantic_param_name: Option<String>,
//     _request_schema_json: Option<String>,
//     request_param_name: Option<String>,
//     background_task_param_name: Option<String>,
//     websocket_param_name: Option<String>,
//     is_websocket: bool,
//     dependencies: Vec<DependencyMeta>,
// }

// type Routes = Arc<StdMutex<Vec<RouteEntry>>>;

// struct ToolEntry { name: String, description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
// struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
// struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

// type Tools = Arc<StdMutex<Vec<ToolEntry>>>;
// type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
// type Prompts = Arc<StdMutex<Vec<PromptEntry>>>;

// fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
// fn parse_pattern(path: &str) -> Vec<Segment> {
//     path_segments(path).into_iter().map(|s| {
//         if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
//         else { Segment::Literal(s.to_string()) }
//     }).collect()
// }

// fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
//     let req_segs = path_segments(path);
//     for (idx, r) in routes.iter().enumerate() {
//         if r.method != method || r.segments.len() != req_segs.len() { continue; }
//         let mut params = HashMap::new(); let mut ok = true;
//         for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
//             match seg {
//                 Segment::Literal(l) => { if l != val { ok = false; break; } },
//                 Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
//             }
//         }
//         if ok { return Some((idx, params)); }
//     }
//     None
// }

// fn parse_query(query: Option<&str>) -> HashMap<String, String> {
//     let mut map = HashMap::new();
//     if let Some(q) = query {
//         for pair in q.split('&').filter(|p| !p.is_empty()) {
//             let mut it = pair.splitn(2, '=');
//             let k = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             let v = urlencoding::decode(it.next().unwrap_or("")).unwrap_or_default().into_owned();
//             map.insert(k, v);
//         }
//     }
//     map
// }

// fn compute_websocket_accept(key: &str) -> String {
//     let mut sha1 = Sha1::new();
//     sha1.update(key.as_bytes());
//     sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
//     general_purpose::STANDARD.encode(sha1.finalize())
// }

// fn generate_openapi(routes: &[RouteEntry]) -> String {
//     let mut paths = serde_json::Map::new();
//     for r in routes {
//         if r.is_websocket { continue; }
//         let mut method_obj = json!({ "responses": { "200": { "description": "Successful Response" } } });
//         if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
//             if r.original_path.contains("upload") {
//                 method_obj["requestBody"] = json!({
//                     "required": true,
//                     "content": {
//                         "multipart/form-data": {
//                             "schema": {
//                                 "type": "object",
//                                 "properties": {
//                                     "document": { "type": "string", "format": "binary", "description": "File to upload" },
//                                     "description": { "type": "string", "description": "Form description field" }
//                                 }
//                             }
//                         }
//                     }
//                 });
//             } else {
//                 method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } } });
//             }
//         }
//         let method_lower = r.method.to_lowercase();
//         if let Some(path_item) = paths.get_mut(&r.original_path) {
//             path_item.as_object_mut().unwrap().insert(method_lower, method_obj);
//         } else {
//             paths.insert(r.original_path.clone(), json!({ method_lower: method_obj }));
//         }
//     }
//     serde_json::to_string(&json!({ "openapi": "3.0.0", "info": { "title": "RustAPI", "version": "0.1.0" }, "paths": paths })).unwrap()
// }

// fn swagger_html() -> String {
//     r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>Swagger UI - RustAPI</title><link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script><script>window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };</script></body></html>"#.to_string()
// }

// #[pyclass(name = "WebSocket")]
// struct PyWebSocket {
//     stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
// }

// #[pymethods]
// impl PyWebSocket {
//     fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 while let Some(msg) = lock.next().await {
//                     if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
//                         return Ok(text.to_string());
//                     }
//                 }
//                 Err(pyo3::exceptions::PyConnectionAbortedError::new_err("Connection closed"))
//             })
//         })
//     }

//     fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
//         let stream = self.stream.clone();
//         let rt = tokio::runtime::Handle::current();
//         py.allow_threads(move || {
//             rt.block_on(async move {
//                 let mut lock = stream.lock().await;
//                 lock.send(tokio_tungstenite::tungstenite::Message::Text(text.into())).await
//                     .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
//                 Ok(())
//             })
//         })
//     }
// }

// #[pyclass(name = "UploadFile")]
// #[derive(Clone)]
// struct PyUploadFile {
//     #[pyo3(get)] filename: String,
//     #[pyo3(get)] content_type: String,
//     file_data: Vec<u8>,
// }

// #[pymethods]
// impl PyUploadFile {
//     fn read(&self, py: Python<'_>) -> PyObject { PyBytes::new_bound(py, &self.file_data).into() }
// }

// #[pyclass]
// struct PyRequest {
//     #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
//     #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
//     #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
//     #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
// }

// #[pyclass(name = "Response")]
// struct PyResponse {
//     #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
// }

// impl Clone for PyResponse {
//     fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
// }

// #[pymethods]
// impl PyResponse {
//     #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
//     fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
// }

// #[pyclass]
// struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

// #[pymethods]
// impl CoroCallback {
//     #[pyo3(signature = (result, error))]
//     fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
//         if let Some(tx) = self.tx.lock().unwrap().take() {
//             if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
//         }
//     }
// }

// #[pyclass]
// struct Engine {
//     routes: Routes, serializer: PyObject, tools: Tools, resources: Resources, prompts: Prompts,
//     schema_fn: PyObject, schedule_coro_fn: PyObject,
// }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl Engine {
//     #[new]
//     fn new(py: Python<'_>) -> PyResult<Self> {
//         let python_code = r#"
// import asyncio, inspect, json, threading
// _engine_loop = asyncio.new_event_loop()
// def _start_engine_loop():
//     asyncio.set_event_loop(_engine_loop)
//     _engine_loop.run_forever()
// threading.Thread(target=_start_engine_loop, daemon=True).start()
// def _schedule_coro(coro, callback):
//     def done_cb(fut):
//         try: callback(fut.result(), None)
//         except Exception as e: callback(None, e)
//     fut = asyncio.run_coroutine_threadsafe(coro, _engine_loop)
//     fut.add_done_callback(done_cb)
// def _serialize_response(val):
//     return json.dumps(val, default=lambda o: o.model_dump() if hasattr(o, "model_dump") else str(o))
// def _schema_from_signature(func):
//     sig = inspect.signature(func)
//     props = {name: {"type": "string"} for name in sig.parameters}
//     return {"type": "object", "properties": props}
// "#;
//         let module = PyModule::from_code_bound(py, python_code, "rustapi_internal.py", "rustapi_internal")?;
//         Ok(Engine {
//             routes: Arc::new(StdMutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
//             schedule_coro_fn: module.getattr("_schedule_coro")?.into(), schema_fn: module.getattr("_schema_from_signature")?.into(),
//             tools: Arc::new(StdMutex::new(Vec::new())), resources: Arc::new(StdMutex::new(Vec::new())), prompts: Arc::new(StdMutex::new(Vec::new())),
//         })
//     }

//     fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: false } }
//     fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path, is_ws: false } }
//     fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path, is_ws: false } }
//     fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
//     fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path, is_ws: false } }
//     fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: true } }
    
//     #[pyo3(signature = (router, prefix="".to_string()))]
//     fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
//         let routes_obj = router.getattr(py, "routes")?;
//         let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
//         for (method, path, func) in routes {
//             let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
//             match method.as_str() {
//                 "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
//                 "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
//                 "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
//             }
//         }
//         Ok(())
//     }

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

//     #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
//     fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
//         let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
//         let safe_workers = if workers < 1 { 1 } else { workers };

//         if (reload || safe_workers > 1) && !is_worker {
//             let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
//             let exit_result: Result<(), PyErr> = py.allow_threads(move || {
//                 let spawn_children = || {
//                     let mut nc = Vec::new();
//                     for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
//                     nc
//                 };
//                 let mut children = spawn_children();
//                 let (tx, rx) = std::sync::mpsc::channel();
//                 let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
//                 loop {
//                     if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
//                         if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
//                             for mut child in children { let _ = child.kill(); let _ = child.wait(); }
//                             children = spawn_children(); continue;
//                         }
//                     }} else { thread::sleep(Duration::from_millis(250)); }
//                     if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
//                 }
//             });
//             if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
//             return Ok(());
//         }

//         let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
//         let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
//         socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
//         socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
//         let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
//         let routes = self.routes.clone(); let tools = self.tools.clone(); let resources = self.resources.clone(); let prompts = self.prompts.clone();
//         let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
//         let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
//         let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

//         let server_handle = thread::spawn(move || {
//             let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
//             rt.block_on(async move {
//                 let make_svc = make_service_fn(move |_| {
//                     let (r, t, res, p, s, sc, sem) = (routes.clone(), tools.clone(), resources.clone(), prompts.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
//                     async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone()))) }
//                 });
//                 let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
//                 let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
//                 if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
//             });
//             let _ = done_tx.send(());
//         });

//         let pending_err = py.allow_threads(move || {
//             loop {
//                 if let Ok(()) = done_rx.try_recv() { return None; }
//                 if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
//                 thread::sleep(Duration::from_millis(100));
//             }
//         });
//         let _ = server_handle.join();
//         if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
//     }
// }

// async fn handle(
//     mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, tools: Tools, resources: Resources, prompts: Prompts, gil_sem: Arc<Semaphore>,
// ) -> Result<HyperResponse<Body>, Infallible> {
//     let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
//     let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
//     for (k, v) in req.headers() {
//         let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
//         if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
//         headers_map.insert(key_str, val_str);
//     }
    
//     // WEBSOCKET UPGRADE CHECK
//     let is_ws_upgrade = req.headers().get(hyper::header::UPGRADE)
//         .and_then(|v| v.to_str().ok())
//         .map(|s| s.eq_ignore_ascii_case("websocket"))
//         .unwrap_or(false);

//     if is_ws_upgrade {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
//         if let Some((idx, _)) = matched {
//             let (handler, ws_param_name) = Python::with_gil(|py| {
//                 let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                 (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
//             });

//             if let Some(ws_name) = ws_param_name {
//                 if let Some(ws_key) = req.headers().get("sec-websocket-key").and_then(|v| v.to_str().ok()) {
//                     let accept_key = compute_websocket_accept(ws_key);
//                     let schedule_coro_ws = schedule_coro.clone();

//                     tokio::spawn(async move {
//                         let res = hyper::upgrade::on(&mut req).await;
//                         if let Ok(upgraded) = res {
//                             let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
//                                 upgraded,
//                                 tokio_tungstenite::tungstenite::protocol::Role::Server,
//                                 None,
//                             ).await;

//                             let ws_obj = Arc::new(TokioMutex::new(ws_stream));
//                             let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());

//                             let coro = Python::with_gil(|py| {
//                                 let kwargs = pyo3::types::PyDict::new_bound(py);
//                                 let _ = kwargs.set_item(ws_name, ws_py_obj.bind(py));
//                                 handler.bind(py).call((), Some(&kwargs)).map(|b| b.unbind()).ok()
//                             });
//                             if let Some(c) = coro {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| {
//                                     if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
//                                         let _ = schedule_coro_ws.bind(py).call1((c, cb));
//                                     }
//                                 });
//                                 let _ = rx.await;
//                             }
//                         }
//                     });

//                     return Ok(HyperResponse::builder()
//                         .status(StatusCode::SWITCHING_PROTOCOLS)
//                         .header(hyper::header::UPGRADE, "websocket")
//                         .header(hyper::header::CONNECTION, "upgrade")
//                         .header("sec-websocket-accept", accept_key)
//                         .body(Body::empty())
//                         .unwrap());
//                 }
//             }
//         }
//     }

//     let mut form_map = HashMap::new();
//     let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
//     let mut body_bytes = Vec::new();
//     let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
//     if let Ok(boundary) = multer::parse_boundary(content_type) {
//         let mut multipart = multer::Multipart::new(req.into_body(), boundary);
//         while let Ok(Some(field)) = multipart.next_field().await {
//             let name = field.name().unwrap_or("").to_string();
//             let file_name = field.file_name().map(|s| s.to_string());
//             let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
//             if let Some(fn_str) = file_name {
//                 let data = field.bytes().await.unwrap_or_default().to_vec();
//                 files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
//             } else {
//                 let text = field.text().await.unwrap_or_default();
//                 form_map.insert(name, text);
//             }
//         }
//     } else {
//         let mut body_stream = req.into_body();
//         while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
//             let chunk = chunk_res.unwrap_or_default();
//             if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
//             body_bytes.extend_from_slice(&chunk);
//         }
//     }
//     let body = String::from_utf8_lossy(&body_bytes).to_string();

//     let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string()); (200, swagger_html(), h)
//     } else if method == "GET" && path == "/openapi.json" {
//         let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (200, spec, h)
//     } else if method == "POST" && path == "/mcp" {
//         let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
//         let req_method = req_json["method"].as_str().unwrap_or("").to_string();
//         let has_id = req_json.get("id").is_some();
//         let msg_id = req_json["id"].clone();
//         let params = req_json.get("params").unwrap_or(&json!({})).clone();

//         if !has_id {
//             let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//             return Ok(HyperResponse::builder().status(202).body(Body::empty()).unwrap());
//         }

//         let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
//         let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

//         let result = if req_method == "initialize" { ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}})) }
//         else if req_method == "ping" { ok(json!({})) }
//         else if req_method == "tools/list" {
//             let guard = tools.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema_json })).collect();
//             ok(json!({"tools": items}))
//         } else if req_method == "resources/list" {
//             let guard = resources.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
//             ok(json!({"resources": items}))
//         } else if req_method == "resources/read" {
//             let uri = params["uri"].as_str().unwrap_or("");
//             let guard = resources.lock().unwrap();
//             if let Some(res_entry) = guard.iter().find(|r| r.uri == uri) {
//                 let content_str = Python::with_gil(|py| {
//                     res_entry.handler.bind(py).call0().map(|v| v.extract::<String>().unwrap_or_default()).unwrap_or_default()
//                 });
//                 ok(json!({"contents": [{"uri": res_entry.uri, "mimeType": res_entry.mime_type, "text": content_str}]}))
//             } else {
//                 err(-32602, &format!("Unknown resource: {}", uri))
//             }
//         } else if req_method == "prompts/list" {
//             let guard = prompts.lock().unwrap();
//             let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
//             ok(json!({"prompts": items}))
//         } else if req_method == "prompts/get" {
//             let name = params["name"].as_str().unwrap_or("");
//             let topic = params.get("arguments").and_then(|a| a.get("topic")).and_then(|v| v.as_str()).unwrap_or("");
//             let guard = prompts.lock().unwrap();
//             if let Some(prompt_entry) = guard.iter().find(|p| p.name == name) {
//                 let content_str = Python::with_gil(|py| {
//                     let kwargs = pyo3::types::PyDict::new_bound(py);
//                     let _ = kwargs.set_item("topic", topic);
//                     prompt_entry.handler.bind(py).call((), Some(&kwargs)).map(|v| v.extract::<String>().unwrap_or_default()).unwrap_or_default()
//                 });
//                 ok(json!({"messages": [{"role": "user", "content": {"type": "text", "text": content_str}}] }))
//             } else {
//                 err(-32602, &format!("Unknown prompt: {}", name))
//             }
//         } else if req_method == "tools/call" {
//             let name = params["name"].as_str().unwrap_or("").to_string();
//             let args_json = params["arguments"].clone();
//             let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t.is_async)));
//             if let Some((handler, is_async)) = tool_opt {
//                 let _permit = gil_sem.acquire().await.ok();
//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
//                         if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));
//                 let (t_status, content, _) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, true).await;
//                 if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
//             } else { err(-32602, &format!("Unknown tool: {}", name)) }
//         } else { err(-32601, &format!("Method not found: {}", req_method)) };
//         let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
//         (200, result, h)
//     } else {
//         let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
//         match matched {
//             Some((idx, path_params)) => {
//                 let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps) = Python::with_gil(|py| {
//                     let guard = routes.lock().unwrap(); let entry = &guard[idx];
//                     (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone())
//                 });

//                 let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
//                 let form_c = form_map.clone(); let files_c = files_map.clone();
                
//                 let mut dependency_error: Option<String> = None;
//                 let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

//                 for dep in deps {
//                     if dep.use_cache && cache.contains_key(&dep.id) {
//                         let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
//                         resolved_args.insert(dep.name.clone(), cached_val); continue;
//                     }
//                     let dep_result_res: Result<PyObject, String> = if dep.is_async {
//                         let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
//                         match coro_res {
//                             Ok(coro) => {
//                                 let (tx, rx) = oneshot::channel();
//                                 Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
//                                 match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
//                             }
//                             Err(e) => Err(e),
//                         }
//                     } else {
//                         let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
//                         tokio::task::spawn_blocking(move || {
//                             let _permit = sem_clone.try_acquire().ok();
//                             Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
//                         }).await.unwrap_or_else(|_| Err("Panic".to_string()))
//                     };

//                     match dep_result_res {
//                         Ok(dep_obj) => {
//                             let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
//                             match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
//                         }
//                         Err(e) => { dependency_error = Some(e); break; }
//                     }
//                 }

//                 if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

//                 let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
//                     Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
//                 } else { None };

//                 let sem_clone = gil_sem.clone();
//                 let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
//                 let bg_param_name_clone = background_task_param_name.clone();

//                 let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
//                     let _permit = sem_clone.try_acquire().ok();
//                     Python::with_gil(|py| -> PyResult<PyObject> {
//                         let kwargs = pyo3::types::PyDict::new_bound(py);
//                         for (k, v) in &path_params_c { kwargs.set_item(k, v)?; }
//                         for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
//                         if let Some(req_name) = request_param_name {
//                             let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
//                             kwargs.set_item(req_name, req_obj)?;
//                         }
//                         if let Some(ref model) = pydantic_model {
//                             let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() } else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
//                             let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
//                             if let Some(model_name) = pydantic_param_name { kwargs.set_item(model_name, instance)?; }
//                         }
//                         if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
//                         handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
//                     })
//                 }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

//                 let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
//                 if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

//                 if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

//                 if let Some(bg_obj) = bg_tasks_obj {
//                     let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
//                     if let Some(tasks) = tasks_list {
//                         let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
//                         tokio::spawn(async move {
//                             for (func, args, kw) in tasks {
//                                 let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
//                                 if is_async {
//                                     let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
//                                     if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
//                                 } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
//                             }
//                         });
//                     }
//                 }
//                 (r_status, r_body, r_headers)
//             }
//             None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
//         }
//     };

//     let mut builder = HyperResponse::builder().status(status);
//     for (k, v) in resp_headers { builder = builder.header(&k, &v); }
//     Ok(builder.body(Body::from(resp_body)).unwrap())
// }

// async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
//     let py_result: PyResult<PyObject> = if is_async {
//         match exec_res {
//             Ok(coro) => {
//                 let (tx, rx) = oneshot::channel();
//                 let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
//                 if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))), } }
//             }
//             Err(e) => Err(e),
//         }
//     } else { exec_res };

//     Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
//         match py_result {
//             Ok(py_obj) => {
//                 if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
//                     let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
//                     let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
//                     return (status, body_str, headers);
//                 }
//                 let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
//                 (200, body_str, HashMap::new())
//             }
//             Err(err) => {
//                 (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
//             }
//         }
//     })
// }

// #[pyclass]
// struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

// #[allow(non_local_definitions)]
// #[pymethods]
// impl RouteDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
//         let mut pydantic_model = None; let mut pydantic_param_name = None; let mut request_schema_json = None;
//         let mut request_param_name = None; let mut background_task_param_name = None; let mut websocket_param_name = None; let mut dependencies = Vec::new(); 
//         if let Ok(params_dict) = params.call_method0("values") {
//             if let Ok(iter) = params_dict.iter() {
//                 for p_res in iter {
//                     if let Ok(p) = p_res {
//                         let param_name: String = p.getattr("name")?.extract()?;
//                         if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
//                         if self.is_ws { websocket_param_name = Some(param_name.clone()); continue; }
//                         if let Ok(annotation) = p.getattr("annotation") {
//                             if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } }
//                             if annotation.hasattr("model_json_schema").unwrap_or(false) {
//                                 pydantic_model = Some(annotation.clone().into()); pydantic_param_name = Some(param_name.clone());
//                                 if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
//                                     if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) { if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); } }
//                                 }
//                                 continue; 
//                             }
//                         }
//                         if let Ok(default_val) = p.getattr("default") {
//                             let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
//                             if is_depends {
//                                 let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
//                                 if !dep_func.is_none() {
//                                     let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
//                                     let dep_id = dep_func.as_ptr() as isize;
//                                     dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         self.routes.lock().unwrap().push(RouteEntry {
//             method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path),
//             handler: func.clone_ref(py), is_async, pydantic_model, pydantic_param_name,
//             _request_schema_json: request_schema_json, request_param_name, background_task_param_name,
//             websocket_param_name, is_websocket: self.is_ws, dependencies,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ToolDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
//         let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
//         self.tools.lock().unwrap().push(ToolEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             schema_json: serde_json::from_str(&schema_str).unwrap(), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct ResourceDecorator { resources: Resources, uri: String, mime_type: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl ResourceDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.resources.lock().unwrap().push(ResourceEntry {
//             uri: self.uri.clone(), description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
//             mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), 
//             is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pyclass]
// struct PromptDecorator { prompts: Prompts, name: Option<String>, _description: Option<String> }
// #[allow(non_local_definitions)]
// #[pymethods]
// impl PromptDecorator {
//     fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
//         self.prompts.lock().unwrap().push(PromptEntry {
//             name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
//             description: self.description.clone().unwrap_or_else(|| py.import_bound("inspect").unwrap().call_method1("getdoc", (func.bind(py),)).unwrap().extract().unwrap_or_default()),
//             handler: func.clone_ref(py), is_async: py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?,
//         });
//         Ok(func)
//     }
// }

// #[pymodule]
// fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<Engine>()?; 
//     m.add_class::<PyRequest>()?; 
//     m.add_class::<PyResponse>()?; 
//     m.add_class::<PyUploadFile>()?;
//     m.add_class::<PyWebSocket>()?;
//     Ok(())
// }



use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyBytes};
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

const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit

#[derive(Clone)]
enum Segment { Literal(String), Param(String) }

struct DependencyMeta {
    name: String, func: Py<PyAny>, _is_async: bool, is_generator: bool, use_cache: bool, id: isize,
}

impl Clone for DependencyMeta {
    fn clone(&self) -> Self {
        Python::with_gil(|py| DependencyMeta {
            name: self.name.clone(), func: self.func.clone_ref(py), _is_async: self._is_async,
            is_generator: self.is_generator, use_cache: self.use_cache, id: self.id,
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
}

type Routes = Arc<StdMutex<Vec<RouteEntry>>>;

struct ToolEntry { name: String, _description: String, schema_json: serde_json::Value, handler: Py<PyAny>, _is_async: bool }
struct ResourceEntry { uri: String, description: String, mime_type: String, handler: Py<PyAny>, _is_async: bool }
struct PromptEntry { name: String, description: String, handler: Py<PyAny>, _is_async: bool }

type Tools = Arc<StdMutex<Vec<ToolEntry>>>;
type Resources = Arc<StdMutex<Vec<ResourceEntry>>>;
type Prompts = Arc<StdMutex<Vec<PromptEntry>>>;

fn path_segments(path: &str) -> Vec<&str> { path.split('/').filter(|s| !s.is_empty()).collect() }
fn parse_pattern(path: &str) -> Vec<Segment> {
    path_segments(path).into_iter().map(|s| {
        if s.starts_with('{') && s.ends_with('}') { Segment::Param(s[1..s.len() - 1].to_string()) } 
        else { Segment::Literal(s.to_string()) }
    }).collect()
}

fn match_route(routes: &[RouteEntry], method: &str, path: &str) -> Option<(usize, HashMap<String, String>)> {
    let req_segs = path_segments(path);
    for (idx, r) in routes.iter().enumerate() {
        if r.method != method || r.segments.len() != req_segs.len() { continue; }
        let mut params = HashMap::new(); let mut ok = true;
        for (seg, val) in r.segments.iter().zip(req_segs.iter()) {
            match seg {
                Segment::Literal(l) => { if l != val { ok = false; break; } },
                Segment::Param(name) => { params.insert(name.clone(), (*val).to_string()); }
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
        if r.method == "POST" || r.method == "PUT" || r.method == "PATCH" {
            if r.original_path.contains("upload") {
                method_obj["requestBody"] = json!({
                    "required": true,
                    "content": {
                        "multipart/form-data": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "document": { "type": "string", "format": "binary", "description": "File to upload" },
                                    "description": { "type": "string", "description": "Form description field" }
                                }
                            }
                        }
                    }
                });
            } else {
                method_obj["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": true } } } });
            }
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
    r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>Swagger UI - RustAPI</title><link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script><script>window.onload = () => { window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };</script></body></html>"#.to_string()
}

#[pyclass(name = "WebSocket")]
struct PyWebSocket {
    stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
}

#[pymethods]
impl PyWebSocket {
    fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
        let stream = self.stream.clone();
        let rt = tokio::runtime::Handle::current();
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
        let rt = tokio::runtime::Handle::current();
        py.allow_threads(move || {
            rt.block_on(async move {
                let mut lock = stream.lock().await;
                lock.send(tokio_tungstenite::tungstenite::Message::Text(text.into())).await
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
    fn read(&self, py: Python<'_>) -> PyObject { PyBytes::new_bound(py, &self.file_data).into() }
}

#[pyclass]
struct PyRequest {
    #[pyo3(get)] method: String, #[pyo3(get)] path: String, #[pyo3(get)] path_params: HashMap<String, String>,
    #[pyo3(get)] query_params: HashMap<String, String>, #[pyo3(get)] headers: HashMap<String, String>,
    #[pyo3(get)] cookies: HashMap<String, String>, #[pyo3(get)] form: HashMap<String, String>, 
    #[pyo3(get)] files: HashMap<String, Vec<PyUploadFile>>, #[pyo3(get)] body: String,
}

#[pyclass(name = "Response")]
struct PyResponse {
    #[pyo3(get)] content: PyObject, #[pyo3(get)] status_code: u16, #[pyo3(get)] headers: HashMap<String, String>,
}

impl Clone for PyResponse {
    fn clone(&self) -> Self { Python::with_gil(|py| PyResponse { content: self.content.clone_ref(py), status_code: self.status_code, headers: self.headers.clone() }) }
}

#[pymethods]
impl PyResponse {
    #[new] #[pyo3(signature = (content, status_code=200, headers=None))]
    fn new(content: PyObject, status_code: u16, headers: Option<HashMap<String, String>>) -> Self { PyResponse { content, status_code, headers: headers.unwrap_or_default() } }
}

#[pyclass]
struct CoroCallback { tx: std::sync::Mutex<Option<oneshot::Sender<Result<PyObject, PyObject>>>> }

#[pymethods]
impl CoroCallback {
    #[pyo3(signature = (result, error))]
    fn __call__(&self, py: Python<'_>, result: PyObject, error: PyObject) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            if error.is_none(py) { let _ = tx.send(Ok(result)); } else { let _ = tx.send(Err(error)); }
        }
    }
}

#[pyclass]
struct Engine {
    routes: Routes, serializer: PyObject, tools: Tools, resources: Resources, prompts: Prompts,
    schema_fn: PyObject, schedule_coro_fn: PyObject,
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
            routes: Arc::new(StdMutex::new(Vec::new())), serializer: module.getattr("_serialize_response")?.into(),
            schedule_coro_fn: module.getattr("_schedule_coro")?.into(), schema_fn: module.getattr("_schema_from_signature")?.into(),
            tools: Arc::new(StdMutex::new(Vec::new())), resources: Arc::new(StdMutex::new(Vec::new())), prompts: Arc::new(StdMutex::new(Vec::new())),
        })
    }

    fn get(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: false } }
    fn post(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "POST".into(), path, is_ws: false } }
    fn put(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PUT".into(), path, is_ws: false } }
    fn delete(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "DELETE".into(), path, is_ws: false } }
    fn patch(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "PATCH".into(), path, is_ws: false } }
    fn websocket(&self, path: String) -> RouteDecorator { RouteDecorator { routes: self.routes.clone(), method: "GET".into(), path, is_ws: true } }
    
    #[pyo3(signature = (router, prefix="".to_string()))]
    fn include_router(&self, py: Python<'_>, router: Py<PyAny>, prefix: String) -> PyResult<()> {
        let routes_obj = router.getattr(py, "routes")?;
        let routes: Vec<(String, String, Py<PyAny>)> = routes_obj.extract(py)?;
        for (method, path, func) in routes {
            let mut full_path = format!("{}{}", prefix, path); full_path = full_path.replace("//", "/");
            match method.as_str() {
                "GET" => { self.get(full_path).__call__(py, func)?; }, "POST" => { self.post(full_path).__call__(py, func)?; },
                "PUT" => { self.put(full_path).__call__(py, func)?; }, "DELETE" => { self.delete(full_path).__call__(py, func)?; },
                "PATCH" => { self.patch(full_path).__call__(py, func)?; }, _ => {}
            }
        }
        Ok(())
    }

    #[pyo3(signature = (name=None, description=None))]
    fn tool(&self, py: Python<'_>, name: Option<String>, description: Option<String>) -> ToolDecorator {
        ToolDecorator { tools: self.tools.clone(), schema_fn: self.schema_fn.clone_ref(py), name }
    }
    #[pyo3(signature = (uri, mime_type=None))]
    fn resource(&self, uri: String, mime_type: Option<String>) -> ResourceDecorator {
        ResourceDecorator { resources: self.resources.clone(), uri, mime_type }
    }
    #[pyo3(signature = (name=None, description=None))]
    fn prompt(&self, name: Option<String>, description: Option<String>) -> PromptDecorator {
        PromptDecorator { prompts: self.prompts.clone(), name }
    }

    #[pyo3(signature = (host="127.0.0.1".to_string(), port=8000, reload=false, workers=1))]
    fn run(&self, py: Python<'_>, host: String, port: u16, reload: bool, workers: usize) -> PyResult<()> {
        let is_worker = std::env::var("RUSTAPI_WORKER").is_ok();
        let safe_workers = if workers < 1 { 1 } else { workers };

        if (reload || safe_workers > 1) && !is_worker {
            let sys = py.import_bound("sys")?; let executable: String = sys.getattr("executable")?.extract()?; let argv: Vec<String> = sys.getattr("argv")?.extract()?;
            let exit_result: Result<(), PyErr> = py.allow_threads(move || {
                let spawn_children = || {
                    let mut nc = Vec::new();
                    for i in 0..safe_workers { nc.push(Command::new(&executable).args(&argv).env("RUSTAPI_WORKER", i.to_string()).spawn().unwrap()); }
                    nc
                };
                let mut children = spawn_children();
                let (tx, rx) = std::sync::mpsc::channel();
                let _watcher_keepalive = if reload { let mut watcher = notify::recommended_watcher(tx).unwrap(); watcher.watch(Path::new("."), RecursiveMode::Recursive).unwrap(); Some(watcher) } else { None };
                loop {
                    if reload { if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(250)) {
                        if event.paths.iter().any(|p| p.extension().map_or(false, |ext| ext == "py")) {
                            for mut child in children { let _ = child.kill(); let _ = child.wait(); }
                            children = spawn_children(); continue;
                        }
                    }} else { thread::sleep(Duration::from_millis(250)); }
                    if let Err(e) = Python::with_gil(|py| py.check_signals()) { for mut child in children { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
                }
            });
            if let Err(err) = exit_result { return Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }); }
            return Ok(());
        }

        let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
        let socket = socket2::Socket::new(if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 }, socket2::Type::STREAM, None).unwrap();
        socket.set_reuse_address(true).unwrap(); #[cfg(unix)] socket.set_reuse_port(true).unwrap();
        socket.bind(&addr.into()).unwrap(); socket.listen(1024).unwrap();
        let std_listener: std::net::TcpListener = socket.into(); std_listener.set_nonblocking(true).unwrap();
        
        let routes = self.routes.clone(); let tools = self.tools.clone(); let resources = self.resources.clone(); let prompts = self.prompts.clone();
        let serializer_arc = Arc::new(self.serializer.clone_ref(py)); let schedule_coro_arc = Arc::new(self.schedule_coro_fn.clone_ref(py));
        let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4); let gil_semaphore = Arc::new(Semaphore::new(num_cpus * 2));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>(); let (done_tx, done_rx) = mpsc::channel::<()>();

        let server_handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(num_cpus * 4).build().unwrap();
            rt.block_on(async move {
                let make_svc = make_service_fn(move |_| {
                    let (r, t, res, p, s, sc, sem) = (routes.clone(), tools.clone(), resources.clone(), prompts.clone(), serializer_arc.clone(), schedule_coro_arc.clone(), gil_semaphore.clone());
                    async move { Ok::<_, Infallible>(service_fn(move |req| handle(req, r.clone(), s.clone(), sc.clone(), t.clone(), res.clone(), p.clone(), sem.clone()))) }
                });
                let server = Server::from_tcp(std_listener).unwrap().http1_keepalive(true).tcp_nodelay(true).serve(make_svc);
                let graceful = server.with_graceful_shutdown(async { let _ = shutdown_rx.await; });
                if let Err(e) = graceful.await { eprintln!("Server error: {e}"); }
            });
            let _ = done_tx.send(());
        });

        let pending_err = py.allow_threads(move || {
            loop {
                if let Ok(()) = done_rx.try_recv() { return None; }
                if let Err(err) = Python::with_gil(|py| py.check_signals()) { let _ = shutdown_tx.send(()); return Some(err); }
                thread::sleep(Duration::from_millis(100));
            }
        });
        let _ = server_handle.join();
        if let Some(err) = pending_err { Python::with_gil(|py| { if err.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) { Ok(()) } else { Err(err) } }) } else { Ok(()) }
    }
}

async fn handle(
    mut req: HyperRequest<Body>, routes: Routes, serializer: Arc<PyObject>, schedule_coro: Arc<PyObject>, tools: Tools, resources: Resources, prompts: Prompts, gil_sem: Arc<Semaphore>,
) -> Result<HyperResponse<Body>, Infallible> {
    let method = req.method().to_string(); let path = req.uri().path().to_string(); let query_params = parse_query(req.uri().query());
    let mut headers_map = HashMap::new(); let mut cookies_map = HashMap::new();
    for (k, v) in req.headers() {
        let key_str = k.as_str().to_string(); let val_str = v.to_str().unwrap_or("").to_string();
        if key_str.eq_ignore_ascii_case("cookie") { for pair in val_str.split(';') { let mut parts = pair.trim().splitn(2, '='); if let (Some(ck), Some(cv)) = (parts.next(), parts.next()) { cookies_map.insert(ck.to_string(), cv.to_string()); } } }
        headers_map.insert(key_str, val_str);
    }
    
    // WEBSOCKET UPGRADE CHECK
    let is_ws_upgrade = req.headers().get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws_upgrade {
        let matched = { let guard = routes.lock().unwrap(); match_route(&guard, "GET", &path) };
        if let Some((idx, _)) = matched {
            let (handler, ws_param_name) = Python::with_gil(|py| {
                let guard = routes.lock().unwrap(); let entry = &guard[idx];
                (entry.handler.clone_ref(py), entry.websocket_param_name.clone())
            });

            if let Some(ws_name) = ws_param_name {
                if let Some(ws_key) = req.headers().get("sec-websocket-key").and_then(|v| v.to_str().ok()) {
                    let accept_key = compute_websocket_accept(ws_key);
                    let schedule_coro_ws = schedule_coro.clone();

                    tokio::spawn(async move {
                        let res = hyper::upgrade::on(&mut req).await;
                        if let Ok(upgraded) = res {
                            let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
                                upgraded,
                                tokio_tungstenite::tungstenite::protocol::Role::Server,
                                None,
                            ).await;

                            let ws_obj = Arc::new(TokioMutex::new(ws_stream));
                            let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());

                            let coro = Python::with_gil(|py| {
                                let kwargs = pyo3::types::PyDict::new_bound(py);
                                let _ = kwargs.set_item(ws_name, ws_py_obj.bind(py));
                                handler.bind(py).call((), Some(&kwargs)).map(|b| b.unbind()).ok()
                            });
                            if let Some(c) = coro {
                                let (tx, rx) = oneshot::channel();
                                Python::with_gil(|py| {
                                    if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) {
                                        let _ = schedule_coro_ws.bind(py).call1((c, cb));
                                    }
                                });
                                let _ = rx.await;
                            }
                        }
                    });

                    return Ok(HyperResponse::builder()
                        .status(StatusCode::SWITCHING_PROTOCOLS)
                        .header(hyper::header::UPGRADE, "websocket")
                        .header(hyper::header::CONNECTION, "upgrade")
                        .header("sec-websocket-accept", accept_key)
                        .body(Body::empty())
                        .unwrap());
                }
            }
        }
    }

    let mut form_map = HashMap::new();
    let mut files_map: HashMap<String, Vec<PyUploadFile>> = HashMap::new();
    let mut body_bytes = Vec::new();
    let content_type = headers_map.get("content-type").map(|s| s.as_str()).unwrap_or("");
    
    if let Ok(boundary) = multer::parse_boundary(content_type) {
        let mut multipart = multer::Multipart::new(req.into_body(), boundary);
        while let Ok(Some(field)) = multipart.next_field().await {
            let name = field.name().unwrap_or("").to_string();
            let file_name = field.file_name().map(|s| s.to_string());
            let c_type = field.content_type().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
            
            if let Some(fn_str) = file_name {
                let data = field.bytes().await.unwrap_or_default().to_vec();
                files_map.entry(name).or_insert_with(Vec::new).push(PyUploadFile { filename: fn_str, content_type: c_type, file_data: data });
            } else {
                let text = field.text().await.unwrap_or_default();
                form_map.insert(name, text);
            }
        }
    } else {
        let mut body_stream = req.into_body();
        while let Some(chunk_res) = hyper::body::HttpBody::data(&mut body_stream).await {
            let chunk = chunk_res.unwrap_or_default();
            if body_bytes.len() + chunk.len() > MAX_PAYLOAD_SIZE { return Ok(HyperResponse::builder().status(413).body(Body::from("Payload Too Large")).unwrap()); }
            body_bytes.extend_from_slice(&chunk);
        }
    }
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    let (status, resp_body, resp_headers) = if method == "GET" && path == "/docs" {
        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "text/html".to_string()); (200, swagger_html(), h)
    } else if method == "GET" && path == "/openapi.json" {
        let spec = { let guard = routes.lock().unwrap(); generate_openapi(&guard) };
        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (200, spec, h)
    } else if method == "POST" && path == "/mcp" {
        let req_json: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
        let req_method = req_json["method"].as_str().unwrap_or("").to_string();
        let has_id = req_json.get("id").is_some();
        let msg_id = req_json["id"].clone();
        let params = req_json.get("params").unwrap_or(&json!({})).clone();

        if !has_id {
            let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
            return Ok(HyperResponse::builder().status(202).body(Body::empty()).unwrap());
        }

        let ok = |res: serde_json::Value| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "result": res}).to_string() };
        let err = |code: i32, msg: &str| -> String { json!({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": msg}}).to_string() };

        let result = if req_method == "initialize" { ok(json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}, "resources": {}, "prompts": {}}, "serverInfo": {"name": "rustapi-mcp", "version": "0.1.0"}})) }
        else if req_method == "ping" { ok(json!({})) }
        else if req_method == "tools/list" {
            let guard = tools.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|t| json!({ "name": t.name, "description": t._description, "inputSchema": t.schema_json })).collect();
            ok(json!({"tools": items}))
        } else if req_method == "resources/list" {
            let guard = resources.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|r| json!({ "uri": r.uri, "name": r.description, "mimeType": r.mime_type })).collect();
            ok(json!({"resources": items}))
        } else if req_method == "resources/read" {
            let uri = params["uri"].as_str().unwrap_or("");
            let guard = resources.lock().unwrap();
            if let Some(res_entry) = guard.iter().find(|r| r.uri == uri) {
                let content_str = Python::with_gil(|py| {
                    res_entry.handler.bind(py).call0().map(|v| v.extract::<String>().unwrap_or_default()).unwrap_or_default()
                });
                ok(json!({"contents": [{"uri": res_entry.uri, "mimeType": res_entry.mime_type, "text": content_str}]}))
            } else {
                err(-32602, &format!("Unknown resource: {}", uri))
            }
        } else if req_method == "prompts/list" {
            let guard = prompts.lock().unwrap();
            let items: Vec<_> = guard.iter().map(|p| json!({ "name": p.name, "description": p.description, "arguments": [] })).collect();
            ok(json!({"prompts": items}))
        } else if req_method == "prompts/get" {
            let name = params["name"].as_str().unwrap_or("");
            let topic = params.get("arguments").and_then(|a| a.get("topic")).and_then(|v| v.as_str()).unwrap_or("");
            let guard = prompts.lock().unwrap();
            if let Some(prompt_entry) = guard.iter().find(|p| p.name == name) {
                let content_str = Python::with_gil(|py| {
                    let kwargs = pyo3::types::PyDict::new_bound(py);
                    let _ = kwargs.set_item("topic", topic);
                    prompt_entry.handler.bind(py).call((), Some(&kwargs)).map(|v| v.extract::<String>().unwrap_or_default()).unwrap_or_default()
                });
                ok(json!({"messages": [{"role": "user", "content": {"type": "text", "text": content_str}}] }))
            } else {
                err(-32602, &format!("Unknown prompt: {}", name))
            }
        } else if req_method == "tools/call" {
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args_json = params["arguments"].clone();
            let tool_opt = Python::with_gil(|py| tools.lock().unwrap().iter().find(|t| t.name == name).map(|t| (t.handler.clone_ref(py), t._is_async)));
            if let Some((handler, is_async_tool)) = tool_opt {
                let _permit = gil_sem.acquire().await.ok();
                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                    Python::with_gil(|py| -> PyResult<PyObject> {
                        let kwargs = py.import_bound("json")?.call_method1("loads", (args_json.to_string(),))?;
                        if let Ok(dict) = kwargs.downcast::<PyDict>() { handler.bind(py).call((), Some(dict)).map(|v| v.into()) } else { handler.bind(py).call0().map(|v| v.into()) }
                    })
                }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker panicked: {e}"))));
                let (t_status, content, _) = execute_python_handler(exec_res, is_async_tool, &serializer, &schedule_coro, true).await;
                if t_status < 400 { ok(json!({"content": [{"type": "text", "text": content}], "isError": false})) } else { ok(json!({"content": [{"type": "text", "text": content}], "isError": true})) }
            } else { err(-32602, &format!("Unknown tool: {}", name)) }
        } else { err(-32601, &format!("Method not found: {}", req_method)) };
        let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string());
        (200, result, h)
    } else {
        let matched = { let guard = routes.lock().unwrap(); match_route(&guard, &method, &path) };
        match matched {
            Some((idx, path_params)) => {
                let (handler, is_async, pydantic_model, pydantic_param_name, request_param_name, background_task_param_name, deps, param_names) = Python::with_gil(|py| {
                    let guard = routes.lock().unwrap(); let entry = &guard[idx];
                    (entry.handler.clone_ref(py), entry.is_async, entry.pydantic_model.as_ref().map(|m| m.clone_ref(py)), entry.pydantic_param_name.clone(), entry.request_param_name.clone(), entry.background_task_param_name.clone(), entry.dependencies.clone(), entry.param_names.clone())
                });

                let method_c = method.clone(); let path_c = path.clone(); let body_c = body.clone(); let headers_c = headers_map.clone(); let cookies_c = cookies_map.clone(); let path_params_c = path_params.clone();
                let form_c = form_map.clone(); let files_c = files_map.clone();
                let param_names_c = param_names.clone();
                
                let mut dependency_error: Option<String> = None;
                let mut resolved_args: HashMap<String, PyObject> = HashMap::new(); let mut cache: HashMap<isize, PyObject> = HashMap::new(); let mut teardown_generators: Vec<PyObject> = Vec::new();

                for dep in deps {
                    if dep.use_cache && cache.contains_key(&dep.id) {
                        let cached_val = Python::with_gil(|py| cache.get(&dep.id).unwrap().clone_ref(py));
                        resolved_args.insert(dep.name.clone(), cached_val); continue;
                    }
                    let dep_result_res: Result<PyObject, String> = if dep._is_async {
                        let coro_res: Result<PyObject, String> = Python::with_gil(|py| dep.func.call0(py).map_err(|e| e.to_string()));
                        match coro_res {
                            Ok(coro) => {
                                let (tx, rx) = oneshot::channel();
                                Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro.bind(py).call1((coro, cb)); } });
                                match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Err(Python::with_gil(|py| err_obj.bind(py).to_string())), Err(_) => Err("Asyncio dropped".to_string()), }
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        let sem_clone = gil_sem.clone(); let dep_func = Python::with_gil(|py| dep.func.clone_ref(py));
                        tokio::task::spawn_blocking(move || {
                            let _permit = sem_clone.try_acquire().ok();
                            Python::with_gil(|py| dep_func.call0(py).map_err(|e| e.to_string()))
                        }).await.unwrap_or_else(|_| Err("Panic".to_string()))
                    };

                    match dep_result_res {
                        Ok(dep_obj) => {
                            let val_res: Result<PyObject, String> = Python::with_gil(|py| { if dep.is_generator { Ok(py.import_bound("builtins").unwrap().call_method1("next", (&dep_obj,)).unwrap().into()) } else { Ok(dep_obj.clone_ref(py)) } });
                            match val_res { Ok(val) => { if dep.is_generator { teardown_generators.push(dep_obj); } if dep.use_cache { cache.insert(dep.id, Python::with_gil(|py| val.clone_ref(py))); } resolved_args.insert(dep.name, val); } Err(e) => { dependency_error = Some(e); break; } }
                        }
                        Err(e) => { dependency_error = Some(e); break; }
                    }
                }

                if let Some(err_msg) = dependency_error { return Ok(HyperResponse::builder().status(500).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"detail":"{}"}}"#, err_msg.replace('"', "'")))).unwrap()); }

                let bg_tasks_obj: Option<PyObject> = if let Some(_) = background_task_param_name {
                    Python::with_gil(|py| py.import_bound("rustapi.background").ok().and_then(|m| m.getattr("BackgroundTasks").ok()).and_then(|cls| cls.call0().ok()).map(|i| i.into()))
                } else { None };

                let sem_clone = gil_sem.clone();
                let bg_obj_for_call = Python::with_gil(|py| bg_tasks_obj.as_ref().map(|obj| obj.clone_ref(py)));
                let bg_param_name_clone = background_task_param_name.clone();

                let exec_res: PyResult<PyObject> = tokio::task::spawn_blocking(move || {
                    let _permit = sem_clone.try_acquire().ok();
                    Python::with_gil(|py| -> PyResult<PyObject> {
                        let kwargs = pyo3::types::PyDict::new_bound(py);
                        for (k, v) in &path_params_c { 
                            if param_names_c.contains(k) {
                                kwargs.set_item(k, v)?; 
                            }
                        }
                        for (k, v) in resolved_args { kwargs.set_item(k, v)?; }
                        if let Some(req_name) = request_param_name {
                            let req_obj = Py::new(py, PyRequest { method: method_c, path: path_c, path_params: path_params_c, query_params, headers: headers_c, cookies: cookies_c, form: form_c, files: files_c, body: body_c.clone() })?;
                            kwargs.set_item(req_name, req_obj)?;
                        }
                        if let Some(ref model) = pydantic_model {
                            let py_dict = if body_c.is_empty() { pyo3::types::PyDict::new_bound(py).into_any() } else { py.import_bound("json")?.call_method1("loads", (&body_c,))?.into_any() };
                            let instance = model.bind(py).call_method1("model_validate", (py_dict,))?;
                            if let Some(model_name) = pydantic_param_name { kwargs.set_item(model_name, instance)?; }
                        }
                        if let (Some(bg_name), Some(bg_obj)) = (bg_param_name_clone, bg_obj_for_call) { kwargs.set_item(bg_name, bg_obj.bind(py))?; }
                        handler.bind(py).call((), Some(&kwargs)).map(|v| v.into())
                    })
                }).await.unwrap_or_else(|e| Err(pyo3::exceptions::PyRuntimeError::new_err(format!("worker thread panicked: {e}"))));

                let (r_status, r_body, mut r_headers) = execute_python_handler(exec_res, is_async, &serializer, &schedule_coro, false).await;
                if !r_headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) { r_headers.insert("Content-Type".to_string(), "application/json".to_string()); }

                if !teardown_generators.is_empty() { tokio::task::spawn_blocking(move || { Python::with_gil(|py| { if let Ok(builtins) = py.import_bound("builtins") { for gen in teardown_generators { let _ = builtins.call_method1("next", (&gen,)); } } }); }); }

                if let Some(bg_obj) = bg_tasks_obj {
                    let tasks_list: Option<Vec<(PyObject, Py<PyTuple>, Py<PyDict>)>> = Python::with_gil(|py| bg_obj.getattr(py, "tasks").ok()?.extract(py).ok());
                    if let Some(tasks) = tasks_list {
                        let schedule_coro_bg = schedule_coro.clone(); let sem_bg = gil_sem.clone();
                        tokio::spawn(async move {
                            for (func, args, kw) in tasks {
                                let is_async = Python::with_gil(|py| py.import_bound("inspect").unwrap().getattr("iscoroutinefunction").unwrap().call1((func.bind(py),)).unwrap().extract::<bool>().unwrap_or(false));
                                if is_async {
                                    let coro: Option<PyObject> = Python::with_gil(|py| func.bind(py).call(args.bind(py), Some(&kw.bind(py))).map(|b| b.unbind()).ok() );
                                    if let Some(c) = coro { let (tx, _rx) = oneshot::channel(); Python::with_gil(|py| { if let Ok(cb) = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) }) { let _ = schedule_coro_bg.bind(py).call1((c, cb)); } }); }
                                } else { let sem_clone = sem_bg.clone(); let _ = tokio::task::spawn_blocking(move || { let _permit = sem_clone.try_acquire().ok(); Python::with_gil(|py| { let _ = func.bind(py).call(args.bind(py), Some(&kw.bind(py))); }); }).await; }
                            }
                        });
                    }
                }
                (r_status, r_body, r_headers)
            }
            None => { let mut h = HashMap::new(); h.insert("Content-Type".to_string(), "application/json".to_string()); (404, r#"{"detail":"Not Found"}"#.to_string(), h) }
        }
    };

    let mut builder = HyperResponse::builder().status(status);
    for (k, v) in resp_headers { builder = builder.header(&k, &v); }
    Ok(builder.body(Body::from(resp_body)).unwrap())
}

async fn execute_python_handler(exec_res: PyResult<PyObject>, is_async: bool, serializer: &PyObject, schedule_coro: &PyObject, raw_string: bool) -> (u16, String, HashMap<String, String>) {
    let py_result: PyResult<PyObject> = if is_async {
        match exec_res {
            Ok(coro) => {
                let (tx, rx) = oneshot::channel();
                let spawn_res = Python::with_gil(|py| -> PyResult<()> { let cb = Py::new(py, CoroCallback { tx: StdMutex::new(Some(tx)) })?; schedule_coro.bind(py).call1((coro, cb))?; Ok(()) });
                if let Err(e) = spawn_res { Err(e) } else { match rx.await { Ok(Ok(res)) => Ok(res), Ok(Err(err_obj)) => Python::with_gil(|py| Err(PyErr::from_value_bound(err_obj.into_bound(py)))), Err(_) => Python::with_gil(|_py| Err(pyo3::exceptions::PyRuntimeError::new_err("Asyncio dropped"))), } }
            }
            Err(e) => Err(e),
        }
    } else { exec_res };

    Python::with_gil(|py| -> (u16, String, HashMap<String, String>) {
        match py_result {
            Ok(py_obj) => {
                if let Ok(resp) = py_obj.downcast_bound::<PyResponse>(py) {
                    let resp_ref = resp.borrow(); let status = resp_ref.status_code; let headers = resp_ref.headers.clone();
                    let body_str = if raw_string { if resp_ref.content.is_none(py) { String::new() } else if let Ok(s) = resp_ref.content.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((&resp_ref.content,)).unwrap().extract().unwrap_or_default() };
                    return (status, body_str, headers);
                }
                let body_str = if raw_string { if py_obj.is_none(py) { String::new() } else if let Ok(s) = py_obj.downcast_bound::<PyString>(py) { s.to_string() } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() } } else { serializer.bind(py).call1((py_obj,)).unwrap().extract().unwrap_or_default() };
                (200, body_str, HashMap::new())
            }
            Err(err) => {
                (500, format!(r#"{{"detail":"{}"}}"#, err.to_string().replace('"', "'")), HashMap::new())
            }
        }
    })
}

#[pyclass]
struct RouteDecorator { routes: Routes, method: String, path: String, is_ws: bool }

#[allow(non_local_definitions)]
#[pymethods]
impl RouteDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let inspect = py.import_bound("inspect")?; let is_async: bool = inspect.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?; let sig = inspect.call_method1("signature", (func.bind(py),))?; let params = sig.getattr("parameters")?;
        let mut pydantic_model = None; let mut pydantic_param_name = None; let mut request_schema_json = None;
        let mut request_param_name = None; let mut background_task_param_name = None; let mut websocket_param_name = None; 
        let mut dependencies = Vec::new(); 
        let mut param_names = Vec::new();

        if let Ok(params_dict) = params.call_method0("values") {
            if let Ok(iter) = params_dict.iter() {
                for p_res in iter {
                    if let Ok(p) = p_res {
                        let param_name: String = p.getattr("name")?.extract()?;
                        param_names.push(param_name.clone());
                        if param_name == "req" || param_name == "request" { request_param_name = Some(param_name); continue; }
                        if self.is_ws { websocket_param_name = Some(param_name.clone()); continue; }
                        if let Ok(annotation) = p.getattr("annotation") {
                            if let Ok(name) = annotation.getattr("__name__") { if name.extract::<String>().unwrap_or_default() == "BackgroundTasks" { background_task_param_name = Some(param_name.clone()); continue; } }
                            if annotation.hasattr("model_json_schema").unwrap_or(false) {
                                pydantic_model = Some(annotation.clone().into()); pydantic_param_name = Some(param_name.clone());
                                if let Ok(schema_dict) = annotation.call_method0("model_json_schema") {
                                    if let Ok(schema_str) = py.import_bound("json")?.call_method1("dumps", (schema_dict,)) { if let Ok(s) = schema_str.extract::<String>() { request_schema_json = Some(s); } }
                                }
                                continue; 
                            }
                        }
                        if let Ok(default_val) = p.getattr("default") {
                            let is_depends = default_val.getattr("__class__").and_then(|cls| cls.getattr("__name__")).and_then(|name| name.extract::<String>()).map(|name| name == "Depends").unwrap_or(false);
                            if is_depends {
                                let dep_func = if let Ok(explicit_dep) = default_val.getattr("dependency") { if explicit_dep.is_none() { p.getattr("annotation").unwrap_or_else(|_| py.None().into_bound(py)) } else { explicit_dep } } else { py.None().into_bound(py) };
                                if !dep_func.is_none() {
                                    let is_dep_async = inspect.getattr("iscoroutinefunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                    let is_dep_gen = inspect.getattr("isgeneratorfunction")?.call1((&dep_func,))?.extract().unwrap_or(false);
                                    let dep_id = dep_func.as_ptr() as isize;
                                    dependencies.push(DependencyMeta { name: param_name.clone(), func: dep_func.into(), _is_async: is_dep_async, is_generator: is_dep_gen, use_cache: true, id: dep_id, });
                                }
                            }
                        }
                    }
                }
            }
        }
        self.routes.lock().unwrap().push(RouteEntry {
            method: self.method.clone(), original_path: self.path.clone(), segments: parse_pattern(&self.path),
            handler: func.clone_ref(py), is_async, pydantic_model, pydantic_param_name,
            _request_schema_json: request_schema_json, request_param_name, background_task_param_name,
            websocket_param_name, is_websocket: self.is_ws, dependencies, param_names,
        });
        Ok(func)
    }
}

#[pyclass]
struct ToolDecorator { tools: Tools, schema_fn: PyObject, name: Option<String> }
#[allow(non_local_definitions)]
#[pymethods]
impl ToolDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let schema_obj = self.schema_fn.bind(py).call1((func.bind(py),))?;
        let schema_str: String = py.import_bound("json")?.call_method1("dumps", (schema_obj,))?.extract()?;
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        self.tools.lock().unwrap().push(ToolEntry {
            name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
            _description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
            schema_json: serde_json::from_str(&schema_str).unwrap(), handler: func.clone_ref(py), 
            _is_async: is_async,
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
        self.resources.lock().unwrap().push(ResourceEntry {
            uri: self.uri.clone(), description: py.import_bound("inspect")?.call_method1("getdoc", (func.bind(py),))?.extract().unwrap_or_default(),
            mime_type: self.mime_type.clone().unwrap_or_else(|| "text/plain".to_string()), handler: func.clone_ref(py), 
            _is_async: is_async,
        });
        Ok(func)
    }
}

#[pyclass]
struct PromptDecorator { prompts: Prompts, name: Option<String> }
#[allow(non_local_definitions)]
#[pymethods]
impl PromptDecorator {
    fn __call__(&self, py: Python<'_>, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let is_async = py.import_bound("inspect")?.getattr("iscoroutinefunction")?.call1((func.bind(py),))?.extract()?;
        let desc = py.import_bound("inspect")?
            .call_method1("getdoc", (func.bind(py),))?
            .extract()
            .unwrap_or_default();
        self.prompts.lock().unwrap().push(PromptEntry {
            name: self.name.clone().unwrap_or_else(|| func.bind(py).getattr("__name__").unwrap().extract().unwrap()), 
            description: desc,
            handler: func.clone_ref(py), _is_async: is_async,
        });
        Ok(func)
    }
}

#[pymodule]
fn _rustapi(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?; 
    m.add_class::<PyRequest>()?; 
    m.add_class::<PyResponse>()?; 
    m.add_class::<PyUploadFile>()?;
    m.add_class::<PyWebSocket>()?;
    Ok(())
}