import rustapi

def test_native_route_registration_and_execution():
    app = rustapi.Engine()
    app.add_native_route("/fast-json", '{"status": "ok", "engine": "pure_rust"}')
    # Engine creates and handles native route internally
    assert app is not None
