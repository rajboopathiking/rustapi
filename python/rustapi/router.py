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

    def _add(self, method: str, path: str):
        def decorator(func):
            self.routes.append((method, path, func))
            return func
        return decorator

    def get(self, path: str):
        return self._add("GET", path)

    def post(self, path: str):
        return self._add("POST", path)

    def put(self, path: str):
        return self._add("PUT", path)

    def delete(self, path: str):
        return self._add("DELETE", path)

    def patch(self, path: str):
        return self._add("PATCH", path)

    def websocket(self, path: str):
        return self._add("WS", path)
