with open("src/lib.rs", "r") as f:
    code = f.read()

# 1. Safely clone the startup handlers using the Python GIL
old_startup = "let startup_hooks = startup_handlers.lock().unwrap().clone();"
new_startup = "let startup_hooks = Python::with_gil(|py| startup_handlers.lock().unwrap().iter().map(|(h, a)| (h.clone_ref(py), *a)).collect::<Vec<_>>());"
code = code.replace(old_startup, new_startup)

# 2. Safely clone the shutdown handlers using the Python GIL
old_shutdown = "let shutdown_hooks = shutdown_handlers.lock().unwrap().clone();"
new_shutdown = "let shutdown_hooks = Python::with_gil(|py| shutdown_handlers.lock().unwrap().iter().map(|(h, a)| (h.clone_ref(py), *a)).collect::<Vec<_>>());"
code = code.replace(old_shutdown, new_shutdown)

# 3. Remove the unused `mut` warning on apply_params
code = code.replace("let mut apply_params =", "let apply_params =")

with open("src/lib.rs", "w") as f:
    f.write(code)

print("✅ Successfully patched Py<PyAny> cloning trait bounds!")
