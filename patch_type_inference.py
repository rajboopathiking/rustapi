with open("src/lib.rs", "r") as f:
    code = f.read()

# Fix the type inference ambiguity by explicitly declaring PyResult<PyObject>
old_line = "let coro = Python::with_gil(|py| handler.bind(py).call0().map(|v| v.into()));"
new_line = "let coro: PyResult<PyObject> = Python::with_gil(|py| handler.bind(py).call0().map(|v| v.into()));"

if old_line in code:
    code = code.replace(old_line, new_line)
    print("✅ Successfully patched type annotations for lifespan coroutines!")
else:
    print("⚠️ Exact match not found. Please verify src/lib.rs contents.")

with open("src/lib.rs", "w") as f:
    f.write(code)
