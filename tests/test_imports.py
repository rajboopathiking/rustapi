def test_public_package_exports_work():
    import rustapi

    assert rustapi.Engine is not None
    assert rustapi.Response is not None
    assert rustapi.HTTPException is not None
    assert rustapi.Depends is not None

    response = rustapi.Response({"ok": True}, status_code=201)
    assert response.status_code == 201
    assert response.content == {"ok": True}
