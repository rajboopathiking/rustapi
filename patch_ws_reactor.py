with open("src/lib.rs", "r") as f:
    code = f.read()

# 1. Add tokio runtime Handle to the PyWebSocket struct
old_struct = """#[pyclass(name = "WebSocket")]
struct PyWebSocket {
    stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
}"""

new_struct = """#[pyclass(name = "WebSocket")]
struct PyWebSocket {
    stream: Arc<TokioMutex<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>>,
    rt: tokio::runtime::Handle,
}"""
code = code.replace(old_struct, new_struct)

# 2. Store the current handle during WebSocket instantiation
old_inst = "let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj }).unwrap().into_any());"
new_inst = "let ws_py_obj = Python::with_gil(|py| Py::new(py, PyWebSocket { stream: ws_obj, rt: tokio::runtime::Handle::current() }).unwrap().into_any());"
code = code.replace(old_inst, new_inst)

# 3. Use the stored handle in receive_text instead of Handle::current()
old_recv = """    fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
        let stream = self.stream.clone();
        let rt = tokio::runtime::Handle::current();"""
new_recv = """    fn receive_text(&self, py: Python<'_>) -> PyResult<String> {
        let stream = self.stream.clone();
        let rt = self.rt.clone();"""
code = code.replace(old_recv, new_recv)

# 4. Use the stored handle in send_text instead of Handle::current()
old_send = """    fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        let stream = self.stream.clone();
        let rt = tokio::runtime::Handle::current();"""
new_send = """    fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        let stream = self.stream.clone();
        let rt = self.rt.clone();"""
code = code.replace(old_send, new_send)

with open("src/lib.rs", "w") as f:
    f.write(code)

print("✅ Successfully patched PyWebSocket to cross thread boundaries safely!")
