from __future__ import annotations


class APIRouter:
    """FastAPI-compatible sub-router.

    Routes collected here are mounted onto the main Engine via::

        app.include_router(router, prefix="/api/v1")

    All five HTTP verbs and WebSocket routes are supported.
    """

    def __init__(self, prefix: str = "", tags: list | None = None):
        self.routes: list[tuple[str, str, object]] = []
        self.prefix = prefix
        self.tags = tags or []

    def _add(self, method: str, path: str, response_model: type | None = None):
        def decorator(func):
            self.routes.append((method, path, func, response_model))
            return func
        return decorator

    def get(self, path: str, response_model: type | None = None):
        return self._add("GET", path, response_model=response_model)

    def post(self, path: str, response_model: type | None = None):
        return self._add("POST", path, response_model=response_model)

    def put(self, path: str, response_model: type | None = None):
        return self._add("PUT", path, response_model=response_model)

    def delete(self, path: str, response_model: type | None = None):
        return self._add("DELETE", path, response_model=response_model)

    def patch(self, path: str, response_model: type | None = None):
        return self._add("PATCH", path, response_model=response_model)

    def websocket(self, path: str):
        return self._add("WS", path)
