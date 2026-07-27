from typing import Callable

class APIRouter:
    def __init__(self, prefix: str = ""):
        self.prefix = prefix
        self.routes = []

    def _add_route(self, method: str, path: str, func: Callable):
        # Prevent double slashes when prefix is used
        full_path = (self.prefix + path).replace("//", "/")
        self.routes.append((method, full_path, func))

    def get(self, path: str):
        def decorator(func: Callable):
            self._add_route("GET", path, func)
            return func
        return decorator

    def post(self, path: str):
        def decorator(func: Callable):
            self._add_route("POST", path, func)
            return func
        return decorator

    def put(self, path: str):
        def decorator(func: Callable):
            self._add_route("PUT", path, func)
            return func
        return decorator

    def delete(self, path: str):
        def decorator(func: Callable):
            self._add_route("DELETE", path, func)
            return func
        return decorator

    def patch(self, path: str):
        def decorator(func: Callable):
            self._add_route("PATCH", path, func)
            return func
        return decorator
