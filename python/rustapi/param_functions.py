from typing import Any, Optional
from .depends import Depends


class Param:
    """Base class for parameter location annotations (Query, Path, Body, Header, Cookie, Form, File)."""

    def __init__(
        self,
        default: Any = ...,
        *,
        alias: Optional[str] = None,
        title: Optional[str] = None,
        description: Optional[str] = None,
        gt: Optional[float] = None,
        ge: Optional[float] = None,
        lt: Optional[float] = None,
        le: Optional[float] = None,
        min_length: Optional[int] = None,
        max_length: Optional[int] = None,
        regex: Optional[str] = None,
        deprecated: Optional[bool] = None,
        **extra: Any,
    ):
        self.default = default
        self.alias = alias
        self.title = title
        self.description = description
        self.gt = gt
        self.ge = ge
        self.lt = lt
        self.le = le
        self.min_length = min_length
        self.max_length = max_length
        self.regex = regex
        self.deprecated = deprecated
        self.extra = extra

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}(default={self.default!r})"


class PathParam(Param): pass
class QueryParam(Param): pass
class BodyParam(Param): pass
class HeaderParam(Param): pass
class CookieParam(Param): pass
class FormParam(Param): pass
class FileParam(Param): pass


def Path(default: Any = ..., **kwargs: Any) -> PathParam:
    return PathParam(default, **kwargs)

def Query(default: Any = ..., **kwargs: Any) -> QueryParam:
    return QueryParam(default, **kwargs)

def Body(default: Any = ..., **kwargs: Any) -> BodyParam:
    return BodyParam(default, **kwargs)

def Header(default: Any = ..., **kwargs: Any) -> HeaderParam:
    return HeaderParam(default, **kwargs)

def Cookie(default: Any = ..., **kwargs: Any) -> CookieParam:
    return CookieParam(default, **kwargs)

def Form(default: Any = ..., **kwargs: Any) -> FormParam:
    return FormParam(default, **kwargs)

def File(default: Any = ..., **kwargs: Any) -> FileParam:
    return FileParam(default, **kwargs)

def Security(dependency: Any = None, *, use_cache: bool = True, scopes: Optional[list[str]] = None) -> Depends:
    return Depends(dependency, use_cache=use_cache)
