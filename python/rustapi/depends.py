from typing import Any, Callable, Optional

class Depends:
    """FastAPI-compatible dependency declaration."""

    def __init__(
        self,
        dependency: Optional[Callable[..., Any]] = None,
        *,
        use_cache: bool = True,
        scope: Optional[str] = "function",
        **kwargs: Any,
    ):
        self.dependency = dependency
        self.use_cache = use_cache
        self.scope = scope
        self.extra = kwargs

    def __repr__(self) -> str:
        dep_name = getattr(self.dependency, "__name__", str(self.dependency))
        return f"Depends({dep_name}, use_cache={self.use_cache}, scope={self.scope!r})"
