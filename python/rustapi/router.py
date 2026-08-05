from __future__ import annotations
from typing import Any, Callable, Dict, List, Optional, Type


class APIRouter:
    """FastAPI-compatible sub-router.

    Routes collected here are mounted onto the main Engine via::

        app.include_router(router, prefix="/api/v1")

    All HTTP verbs, WebSockets, and frontend SPA serving are supported.
    """

    def __init__(
        self,
        prefix: str = "",
        tags: Optional[List[str]] = None,
        dependencies: Optional[List[Any]] = None,
        default_response_class: Optional[Type[Any]] = None,
        responses: Optional[Dict[Any, Any]] = None,
        callbacks: Optional[List[Any]] = None,
        routes: Optional[List[Any]] = None,
        redirect_slashes: bool = True,
        default: Optional[Callable[..., Any]] = None,
        dependency_overrides_provider: Optional[Any] = None,
        route_class: Optional[Type[Any]] = None,
        on_startup: Optional[List[Callable[..., Any]]] = None,
        on_shutdown: Optional[List[Callable[..., Any]]] = None,
        lifespan: Optional[Callable[..., Any]] = None,
        deprecated: Optional[bool] = None,
        include_in_schema: bool = True,
        **extra: Any,
    ):
        self.routes: List[tuple] = routes or []
        self.prefix = prefix.rstrip("/")
        self.tags = tags or []
        self.dependencies = dependencies or []
        self.responses = responses or {}
        self.extra = extra

    def _add(self, method: str, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        def decorator(func: Callable[..., Any]):
            self.routes.append((method, path, func, response_model, kwargs))
            return func
        return decorator

    def include_router(
        self,
        router: APIRouter,
        prefix: str = "",
        tags: Optional[List[str]] = None,
        dependencies: Optional[List[Any]] = None,
        **kwargs: Any,
    ):
        """Include a sub-router into this APIRouter instance."""
        base_prefix = f"{prefix.rstrip('/')}{router.prefix}"
        merged_tags = list(dict.fromkeys((tags or []) + (router.tags or [])))
        merged_deps = (dependencies or []) + (router.dependencies or [])

        for item in router.routes:
            method, sub_path, func, response_model, route_kwargs = item
            full_path = f"{base_prefix}{sub_path}".replace("//", "/")
            if not full_path.startswith("/"):
                full_path = f"/{full_path}"

            r_kw = route_kwargs.copy() if route_kwargs else {}
            r_tags = list(dict.fromkeys(merged_tags + r_kw.get("tags", [])))
            r_deps = merged_deps + r_kw.get("dependencies", [])
            if r_tags:
                r_kw["tags"] = r_tags
            if r_deps:
                r_kw["dependencies"] = r_deps

            self.routes.append((method, full_path, func, response_model, r_kw))

    def get(self, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        return self._add("GET", path, response_model=response_model, **kwargs)

    def post(self, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        return self._add("POST", path, response_model=response_model, **kwargs)

    def put(self, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        return self._add("PUT", path, response_model=response_model, **kwargs)

    def delete(self, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        return self._add("DELETE", path, response_model=response_model, **kwargs)

    def patch(self, path: str, response_model: Optional[Type[Any]] = None, **kwargs: Any):
        return self._add("PATCH", path, response_model=response_model, **kwargs)

    def websocket(self, path: str, **kwargs: Any):
        return self._add("WS", path, **kwargs)

    def frontend(self, path: str = "/", directory: str = "dist"):
        """Serve a built static frontend app (e.g. Vite, React, Vue, Svelte output directory)."""
        from .staticfiles import StaticFiles
        handler = StaticFiles(directory=directory, html=True)
        norm_path = path.rstrip("/")
        wildcard_path = f"{norm_path}/{{file_path:path}}" if norm_path else "/{file_path:path}"
        root_path = norm_path if norm_path else "/"

        self.get(root_path)(lambda: handler(""))
        self.get(wildcard_path)(lambda file_path="": handler(file_path))
