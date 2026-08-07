from typing import Any, Dict, List, Optional, Tuple
import inspect
import json as _json
from .status import status
from pydantic import BaseModel
from ._rustapi import (
    Engine as _RustEngine,
    Route,
    PyRequest,
    UploadFile,
    WebSocket,
    StreamingResponse,
    encode_jwt,
    decode_jwt,
    hash_password,
    verify_password,
    render_template,
)
from .exceptions import HTTPException, WebSocketException, WebSocketDisconnect
from .depends import Depends
from .router import APIRouter
from .background import BackgroundTasks
from .sse import EventSourceResponse, ServerSentEvent, format_sse_event
from .encoders import jsonable_encoder
from .param_functions import (
    Body,
    Cookie,
    File,
    Form,
    Header,
    Path,
    Query,
    Security,
)

from .responses import FileResponse
from . import responses, middleware, security, openapi


class Engine(_RustEngine):
    """RustAPI Engine application class wrapping the Rust Tokio core engine."""

    def __init__(self, *args: Any, **kwargs: Any):
        super().__init__()
        self._route_metadata: List[Tuple[str, str, Any, List[str], Dict[str, Any]]] = []
        self._openapi_schema: Optional[Dict[str, Any]] = None

    def _add_route_decorator(self, method: str, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        merged_kwargs = dict(kwargs)
        merged_kwargs['response_model'] = response_model
        if tags:
            merged_kwargs['tags'] = tags

        rust_dec = getattr(super(), method.lower())(path)

        def decorator(func: Any):
            handler_to_register = func
            if response_model:
                import functools
                from typing import get_origin, get_args

                actual_func = func

                @functools.wraps(actual_func)
                def response_model_wrapper(*args, **kw):
                    res = actual_func(*args, **kw)
                    if res is not None:
                        if isinstance(response_model, type) and issubclass(response_model, BaseModel):
                            if isinstance(res, list):
                                return [response_model.model_validate(item).model_dump() for item in res]
                            elif isinstance(res, dict):
                                return response_model.model_validate(res).model_dump()
                        elif hasattr(response_model, "__args__") and get_origin(response_model) in (list, tuple, set):
                            elem_cls = get_args(response_model)[0]
                            if isinstance(elem_cls, type) and issubclass(elem_cls, BaseModel) and isinstance(res, list):
                                return [elem_cls.model_validate(item).model_dump() for item in res]
                    return res

                handler_to_register = response_model_wrapper

            res = rust_dec(handler_to_register)
            self._route_metadata.append((method, path, func, tags or [], merged_kwargs))
            self._openapi_schema = None
            return res

        return decorator

    def get(self, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        return self._add_route_decorator("GET", path, response_model=response_model, tags=tags, **kwargs)

    def post(self, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        return self._add_route_decorator("POST", path, response_model=response_model, tags=tags, **kwargs)

    def put(self, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        return self._add_route_decorator("PUT", path, response_model=response_model, tags=tags, **kwargs)

    def delete(self, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        return self._add_route_decorator("DELETE", path, response_model=response_model, tags=tags, **kwargs)

    def patch(self, path: str, response_model: Optional[Any] = None, tags: Optional[List[str]] = None, **kwargs: Any):
        return self._add_route_decorator("PATCH", path, response_model=response_model, tags=tags, **kwargs)


class FastAPI(Engine):
    """FastAPI-compatible application class wrapping the Rust Tokio core engine."""

    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)

    def __init__(
        self,
        title: str = "RustAPI",
        description: str = "",
        version: str = "0.1.0",
        openapi_url: Optional[str] = "/openapi.json",
        docs_url: Optional[str] = "/docs",
        redoc_url: Optional[str] = "/redoc",
        swagger_ui_oauth2_redirect_url: Optional[str] = "/docs/oauth2-redirect",
        swagger_ui_parameters: Optional[Dict[str, Any]] = None,
        swagger_ui_init_oauth: Optional[Dict[str, Any]] = None,
        servers: Optional[List[Dict[str, Any]]] = None,
        tags: Optional[List[Dict[str, Any]]] = None,
        terms_of_service: Optional[str] = None,
        contact: Optional[Dict[str, Any]] = None,
        license_info: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ):
        super().__init__()
        self.title = title
        self.description = description
        self.version = version
        self.openapi_url = openapi_url
        self.docs_url = docs_url
        self.redoc_url = redoc_url
        self.swagger_ui_oauth2_redirect_url = swagger_ui_oauth2_redirect_url
        self.swagger_ui_parameters = swagger_ui_parameters
        self.swagger_ui_init_oauth = swagger_ui_init_oauth
        self.servers = servers
        self.openapi_tags = tags
        self.terms_of_service = terms_of_service
        self.contact = contact
        self.license_info = license_info
        self.exception_handlers: Dict[Any, Any] = {}
        self.middlewares: list = []
        self.dependency_overrides: Dict[Any, Any] = {}

    def add_middleware(self, middleware_cls: type, **kwargs: Any):
        """Add middleware (such as CORSMiddleware) to application configuration."""
        self.middlewares.append((middleware_cls, kwargs))

    def include_router(self, router: 'APIRouter', prefix: str = "", tags: Optional[List[str]] = None, **kwargs: Any) -> None:
        """Include an APIRouter and capture route metadata for OpenAPI generation."""
        # Call the parent Rust Engine include_router to register routes
        super().include_router(router, prefix, tags=tags, **kwargs)

        # Capture route metadata for OpenAPI generation
        router_prefix = getattr(router, 'prefix', '')
        router_tags = getattr(router, 'tags', []) + (tags or [])
        router_deps = getattr(router, 'dependencies', [])

        for route_entry in getattr(router, 'routes', []):
            if len(route_entry) >= 5:
                method, sub_path, handler, response_model, route_kwargs = route_entry
            elif len(route_entry) >= 3:
                method, sub_path, handler = route_entry[0], route_entry[1], route_entry[2]
                response_model = route_entry[3] if len(route_entry) > 3 else None
                route_kwargs = route_entry[4] if len(route_entry) > 4 else {}
            else:
                continue

            full_path = f"{prefix}{router_prefix}{sub_path}".replace("//", "/")
            if not full_path.startswith("/"):
                full_path = f"/{full_path}"

            # Merge tags from router and route
            route_tags = list(dict.fromkeys(
                router_tags + (route_kwargs.get('tags', []) if route_kwargs else [])
            ))
            # Merge dependencies from router and route
            deps = list(router_deps) + (route_kwargs.get('dependencies', []) if route_kwargs else [])
            merged_kwargs = dict(route_kwargs or {})
            merged_kwargs['dependencies'] = deps
            merged_kwargs['response_model'] = response_model

            self._route_metadata.append((method, full_path, handler, route_tags, merged_kwargs))

        # Clear cached OpenAPI schema since routes changed
        self._openapi_schema = None

    @staticmethod
    def _detect_security_schemes_from_handler(handler: Any, _visited: set | None = None) -> List[Tuple[str, Dict[str, Any]]]:
        """Inspect a handler's signature for security dependencies (HTTPBearer, etc.).

        Recursively traverses the dependency chain to find security schemes
        at any depth (e.g., route → get_admin_user → get_current_user → HTTPBearer).
        """
        from .depends import Depends
        from .security.base import SecurityBase
        schemes: List[Tuple[str, Dict[str, Any]]] = []

        if _visited is None:
            _visited = set()

        handler_id = id(handler)
        if handler_id in _visited:
            return schemes
        _visited.add(handler_id)

        try:
            sig = inspect.signature(handler)
        except (ValueError, TypeError):
            return schemes

        for pname, param in sig.parameters.items():
            default = param.default
            dep_func = None

            if isinstance(default, Depends):
                dep_func = default.dependency
            elif isinstance(default, SecurityBase):
                dep_func = default

            if dep_func is None:
                continue

            # Check if dep_func itself is a security scheme
            if isinstance(dep_func, SecurityBase):
                scheme_info = FastAPI._security_base_to_openapi(dep_func)
                if scheme_info:
                    schemes.append(scheme_info)
                continue

            # Recursively inspect callable dependencies for security schemes
            if callable(dep_func):
                sub_schemes = FastAPI._detect_security_schemes_from_handler(dep_func, _visited)
                schemes.extend(sub_schemes)

        return schemes

    @staticmethod
    def _security_base_to_openapi(sec: Any) -> Optional[Tuple[str, Dict[str, Any]]]:
        """Convert a SecurityBase instance to an OpenAPI securityScheme entry."""
        scheme_name = getattr(sec, 'scheme_name', None) or sec.__class__.__name__
        cls_name = sec.__class__.__name__

        if cls_name == 'HTTPBearer':
            scheme_def: Dict[str, Any] = {"type": "http", "scheme": "bearer"}
            bearer_fmt = getattr(sec, 'bearerFormat', None)
            if bearer_fmt:
                scheme_def["bearerFormat"] = bearer_fmt
            desc = getattr(sec, 'description', None)
            if desc:
                scheme_def["description"] = desc
            return (scheme_name, scheme_def)
        elif cls_name == 'HTTPBasic':
            return (scheme_name, {"type": "http", "scheme": "basic"})
        elif cls_name == 'HTTPDigest':
            return (scheme_name, {"type": "http", "scheme": "digest"})
        elif cls_name in ('APIKeyHeader', 'APIKeyQuery', 'APIKeyCookie'):
            in_loc = {'APIKeyHeader': 'header', 'APIKeyQuery': 'query', 'APIKeyCookie': 'cookie'}[cls_name]
            name = getattr(sec, 'name', 'api_key')
            return (scheme_name, {"type": "apiKey", "name": name, "in": in_loc})
        elif cls_name == 'OAuth2PasswordBearer':
            token_url = getattr(sec, 'tokenUrl', '/token')
            return (scheme_name, {
                "type": "oauth2",
                "flows": {"password": {"tokenUrl": token_url, "scopes": {}}}
            })
        elif cls_name == 'OAuth2AuthorizationCodeBearer':
            auth_url = getattr(sec, 'authorizationUrl', '')
            token_url = getattr(sec, 'tokenUrl', '')
            return (scheme_name, {
                "type": "oauth2",
                "flows": {"authorizationCode": {"authorizationUrl": auth_url, "tokenUrl": token_url, "scopes": {}}}
            })
        elif cls_name == 'OpenIdConnect':
            openid_url = getattr(sec, 'openIdConnectUrl', '')
            return (scheme_name, {"type": "openIdConnect", "openIdConnectUrl": openid_url})

        return None

    def openapi(self) -> Dict[str, Any]:
        """Generate OpenAPI 3.0.0 specification with security schemes."""
        if self._openapi_schema:
            return self._openapi_schema

        info: Dict[str, Any] = {
            "title": self.title,
            "version": self.version,
        }
        if self.description:
            info["description"] = self.description
        if self.terms_of_service:
            info["termsOfService"] = self.terms_of_service
        if self.contact:
            info["contact"] = self.contact
        if self.license_info:
            info["license"] = self.license_info

        schema: Dict[str, Any] = {
            "openapi": "3.0.0",
            "info": info,
            "paths": {},
        }

        if self.servers:
            schema["servers"] = self.servers
        if self.openapi_tags:
            schema["tags"] = self.openapi_tags

        security_schemes: Dict[str, Dict[str, Any]] = {}
        components_schemas: Dict[str, Any] = {}
        paths: Dict[str, Dict[str, Any]] = {}

        VALIDATION_ERROR_DEFINITION = {
            "title": "ValidationError",
            "type": "object",
            "properties": {
                "loc": {
                    "title": "Location",
                    "type": "array",
                    "items": {"anyOf": [{"type": "string"}, {"type": "integer"}]},
                },
                "msg": {"title": "Message", "type": "string"},
                "type": {"title": "Error Type", "type": "string"},
                "input": {"title": "Input"},
                "ctx": {"title": "Context", "type": "object"},
            },
            "required": ["loc", "msg", "type"],
        }

        HTTP_VALIDATION_ERROR_DEFINITION = {
            "title": "HTTPValidationError",
            "type": "object",
            "properties": {
                "detail": {
                    "title": "Detail",
                    "type": "array",
                    "items": {"$ref": "#/components/schemas/ValidationError"},
                }
            },
            "type": "object",
            "title": "HTTPValidationError",
        }

        def _fix_refs(obj: Any) -> Any:
            if isinstance(obj, dict):
                res = {}
                for k, v in obj.items():
                    if k == "$ref" and isinstance(v, str) and v.startswith("#/$defs/"):
                        res[k] = "#/components/schemas/" + v[len("#/$defs/"):]
                    else:
                        res[k] = _fix_refs(v)
                return res
            elif isinstance(obj, list):
                return [_fix_refs(x) for x in obj]
            return obj

        def _extract_model_schema(model_cls: Any) -> Optional[str]:
            if not model_cls or not hasattr(model_cls, "__name__"):
                return None
            name = getattr(model_cls, "__name__", "Model")
            if name not in components_schemas:
                if hasattr(model_cls, "model_json_schema"):
                    schema_data = model_cls.model_json_schema()
                elif hasattr(model_cls, "schema"):
                    schema_data = model_cls.schema()
                else:
                    return None

                defs = schema_data.pop("$defs", None)
                if defs and isinstance(defs, dict):
                    for def_name, def_schema in defs.items():
                        if def_name not in components_schemas:
                            components_schemas[def_name] = _fix_refs(def_schema)

                components_schemas[name] = _fix_refs(schema_data)
            return name

        metadata_routes = list(self._route_metadata)
        registered_keys = {(m.lower(), p) for m, p, _, _, _ in metadata_routes}

        try:
            for r in self.routes:
                for m in r.methods:
                    m_lower = m.lower()
                    if (m_lower, r.path) not in registered_keys:
                        kwargs = {
                            'summary': r.summary,
                            'description': r.description,
                            'dependencies': r.dependencies,
                            'tags': r.tags,
                        }
                        metadata_routes.append((m, r.path, r.endpoint, r.tags, kwargs))
                        registered_keys.add((m_lower, r.path))
        except Exception:
            pass

        for method, path, handler, tags, kwargs in metadata_routes:
            method_lower = method.lower()
            if method_lower == 'ws':
                continue

            # Build operation object
            handler_name = getattr(handler, '__name__', 'handler')
            import re
            clean_path = re.sub(r'[\{\}/]', '_', path).strip('_')

            response_200: Dict[str, Any] = {"description": "Successful Response"}
            resp_model = kwargs.get('response_model')
            if resp_model:
                model_ref_name = _extract_model_schema(resp_model)
                if model_ref_name:
                    response_200["content"] = {
                        "application/json": {
                            "schema": {"$ref": f"#/components/schemas/{model_ref_name}"}
                        }
                    }

            operation: Dict[str, Any] = {
                "summary": kwargs.get('summary') or handler_name.replace('_', ' ').title(),
                "operationId": f"{handler_name}_{clean_path}_{method_lower}",
                "responses": kwargs.get('responses') or {
                    "200": response_200,
                    "422": {
                        "description": "Validation Error",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/HTTPValidationError"}
                            }
                        },
                    },
                },
            }
            if "ValidationError" not in components_schemas:
                components_schemas["ValidationError"] = VALIDATION_ERROR_DEFINITION
                components_schemas["HTTPValidationError"] = HTTP_VALIDATION_ERROR_DEFINITION

            if kwargs.get('description'):
                operation["description"] = kwargs['description']
            elif getattr(handler, '__doc__', None):
                operation["description"] = handler.__doc__.strip()

            if tags:
                operation["tags"] = tags

            if kwargs.get('deprecated'):
                operation["deprecated"] = True

            # Inspect handler signature
            try:
                sig = inspect.signature(handler)
            except (ValueError, TypeError):
                sig = None

            # Parse path parameters with type resolution from signature
            path_params = re.findall(r'\{(\w+)\}', path)
            parameters: List[Dict[str, Any]] = []
            for pp in path_params:
                pp_schema: Dict[str, Any] = {"type": "string"}
                if sig and pp in sig.parameters:
                    pp_ann = sig.parameters[pp].annotation
                    if pp_ann is int:
                        pp_schema = {"type": "integer"}
                    elif pp_ann is float:
                        pp_schema = {"type": "number"}
                    elif pp_ann is bool:
                        pp_schema = {"type": "boolean"}

                parameters.append({
                    "name": pp,
                    "in": "path",
                    "required": True,
                    "schema": pp_schema,
                })

            # Inspect handler signature for query parameters and multipart file/form fields
            is_multipart = False
            multipart_props: Dict[str, Any] = {}
            required_multipart: List[str] = []
            body_pydantic_ref: Optional[str] = None

            if sig:
                skip_params = {'self', 'cls', 'request', 'req', 'session', 'db',
                               'current_user', 'token_auth', 'return'}
                for pname, param in sig.parameters.items():
                    if pname in skip_params:
                        continue
                    if pname in path_params:
                        continue
                    from .depends import Depends as _Dep
                    from .security.base import SecurityBase as _SB

                    is_dep_object = (
                        isinstance(param.default, (_Dep, _SB))
                        or getattr(param.default, "__class__", None).__name__ in ("Depends", "Security", "HTTPBearer", "OAuth2PasswordBearer", "APIKeyHeader", "APIKeyQuery", "APIKeyCookie", "HTTPBasic", "HTTPDigest")
                        or hasattr(param.default, "dependency")
                    )
                    if is_dep_object:
                        continue

                    from typing import get_origin, get_args
                    from .param_functions import FileParam, FormParam

                    ann = param.annotation
                    ann_str = str(ann)
                    ann_name = getattr(ann, '__name__', ann_str)
                    origin = get_origin(ann)
                    args = get_args(ann)

                    # Check if parameter is a Pydantic model for JSON request body
                    if isinstance(ann, type) and (hasattr(ann, "model_json_schema") or hasattr(ann, "schema")):
                        body_pydantic_ref = _extract_model_schema(ann)
                        continue

                    is_sequence = (
                        origin in (list, tuple, set)
                        or 'list' in ann_str.lower()
                        or 'sequence' in ann_str.lower()
                    )

                    is_file_param = (
                        isinstance(param.default, FileParam)
                        or getattr(param.default, '__class__', None).__name__ == 'FileParam'
                        or ann_name == 'UploadFile'
                        or 'UploadFile' in ann_str
                        or any('UploadFile' in str(a) for a in args)
                    )

                    is_form_param = (
                        isinstance(param.default, FormParam)
                        or getattr(param.default, '__class__', None).__name__ == 'FormParam'
                    )

                    if is_file_param or is_form_param:
                        is_multipart = True
                        if is_file_param:
                            if is_sequence:
                                multipart_props[pname] = {
                                    "type": "array",
                                    "items": {"type": "string", "format": "binary"},
                                }
                            else:
                                multipart_props[pname] = {"type": "string", "format": "binary"}
                        else:
                            f_schema: Dict[str, Any] = {"type": "string"}
                            if ann is int:
                                f_schema = {"type": "integer"}
                            elif ann is float:
                                f_schema = {"type": "number"}
                            elif ann is bool:
                                f_schema = {"type": "boolean"}

                            if is_sequence:
                                multipart_props[pname] = {
                                    "type": "array",
                                    "items": f_schema,
                                }
                            else:
                                multipart_props[pname] = f_schema

                        default_val = getattr(param.default, 'default', param.default)
                        if param.default is inspect.Parameter.empty or default_val is ...:
                            required_multipart.append(pname)
                        continue

                    p_schema: Dict[str, Any] = {"type": "string"}
                    if ann is not inspect.Parameter.empty:
                        if ann is int:
                            p_schema = {"type": "integer"}
                        elif ann is float:
                            p_schema = {"type": "number"}
                        elif ann is bool:
                            p_schema = {"type": "boolean"}

                    def_val = getattr(param.default, 'default', param.default)
                    if def_val is not inspect.Parameter.empty and def_val is not ...:
                        if isinstance(def_val, (int, float, str, bool, list, dict, type(None))):
                            p_schema["default"] = def_val

                    p_entry: Dict[str, Any] = {
                        "name": pname,
                        "in": "query",
                        "required": param.default is inspect.Parameter.empty,
                        "schema": p_schema,
                    }
                    parameters.append(p_entry)

            if parameters:
                operation["parameters"] = parameters

            # Request body for mutation methods
            if method_lower in ('post', 'put', 'patch'):
                if is_multipart or 'upload' in path.lower() or 'file' in path.lower():
                    if not multipart_props:
                        multipart_props = {"file": {"type": "string", "format": "binary"}}

                    req_schema: Dict[str, Any] = {
                        "type": "object",
                        "properties": multipart_props,
                    }
                    if required_multipart:
                        req_schema["required"] = required_multipart

                    operation["requestBody"] = {
                        "required": True,
                        "content": {
                            "multipart/form-data": {
                                "schema": req_schema
                            }
                        },
                    }
                else:
                    body_schema_obj = (
                        {"$ref": f"#/components/schemas/{body_pydantic_ref}"}
                        if body_pydantic_ref
                        else {"type": "object"}
                    )
                    operation["requestBody"] = {
                        "required": True,
                        "content": {
                            "application/json": {
                                "schema": body_schema_obj
                            }
                        },
                    }

            # Detect security from handler and its dependency chain
            handler_schemes = self._detect_security_schemes_from_handler(handler)

            # Also detect from explicit router-level dependencies
            for dep in kwargs.get('dependencies', []):
                from .depends import Depends as _Dep
                dep_func = dep.dependency if isinstance(dep, _Dep) else dep
                from .security.base import SecurityBase as _SB
                if isinstance(dep_func, _SB):
                    si = self._security_base_to_openapi(dep_func)
                    if si:
                        handler_schemes.append(si)

            if handler_schemes:
                op_security: List[Dict[str, List[str]]] = []
                for sname, sdef in handler_schemes:
                    security_schemes[sname] = sdef
                    if {sname: []} not in op_security:
                        op_security.append({sname: []})
                operation["security"] = op_security

            # Add to paths
            if path not in paths:
                paths[path] = {}
            paths[path][method_lower] = operation

        schema["paths"] = paths

        # Add components (schemas and securitySchemes)
        components: Dict[str, Any] = {}
        if components_schemas:
            components["schemas"] = components_schemas
        if security_schemes:
            components["securitySchemes"] = security_schemes
        if components:
            schema["components"] = components

        self._openapi_schema = schema

        # Register native routes so Rust Tokio/Hyper server serves them natively
        if self.openapi_url:
            self.add_native_route(
                self.openapi_url,
                _json.dumps(schema, ensure_ascii=False),
                method="GET",
                status_code=200,
                content_type="application/json",
            )
        if self.docs_url:
            self.add_native_route(
                self.docs_url,
                self._get_swagger_ui_html(),
                method="GET",
                status_code=200,
                content_type="text/html; charset=utf-8",
            )
        if self.redoc_url:
            self.add_native_route(
                self.redoc_url,
                self._get_redoc_html(),
                method="GET",
                status_code=200,
                content_type="text/html; charset=utf-8",
            )
        if self.swagger_ui_oauth2_redirect_url:
            from .openapi.docs import get_swagger_ui_oauth2_redirect_html
            redirect_resp = get_swagger_ui_oauth2_redirect_html()
            redirect_html = getattr(redirect_resp, 'content', str(redirect_resp))
            if isinstance(redirect_html, bytes):
                redirect_html = redirect_html.decode('utf-8')
            self.add_native_route(
                self.swagger_ui_oauth2_redirect_url,
                redirect_html,
                method="GET",
                status_code=200,
                content_type="text/html; charset=utf-8",
            )

        return schema

    def run(self, host: str = "127.0.0.1", port: int = 8000, reload: bool = False, workers: int = 1):
        """Run application with native Rust Tokio/Hyper server."""
        self.openapi()
        super().run(host=host, port=port, reload=reload, workers=workers)

    def _get_swagger_ui_html(self) -> str:
        """Generate Swagger UI HTML with Authorize button support."""
        params: Dict[str, Any] = {
            "dom_id": "#swagger-ui",
            "layout": "BaseLayout",
            "deepLinking": True,
            "showExtensions": True,
            "showCommonExtensions": True,
            "persistAuthorization": True,
            "filter": True,
        }
        if self.swagger_ui_parameters:
            params.update(self.swagger_ui_parameters)

        params_js = ", ".join(
            f"{k}: {_json.dumps(v)}" for k, v in params.items()
        )

        oauth_init = ""
        if self.swagger_ui_init_oauth:
            oauth_init = f"ui.initOAuth({_json.dumps(self.swagger_ui_init_oauth)})"

        oauth2_redirect = ""
        if self.swagger_ui_oauth2_redirect_url:
            oauth2_redirect = f"oauth2RedirectUrl: window.location.origin + '{self.swagger_ui_oauth2_redirect_url}',"

        return f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{self.title} - Swagger UI</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css" />
    <link rel="icon" type="image/png" href="https://fastapi.tiangolo.com/img/favicon.png" />
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
    const ui = SwaggerUIBundle({{
        url: "{self.openapi_url}",
        {oauth2_redirect}
        {params_js},
        presets: [
            SwaggerUIBundle.presets.apis,
            SwaggerUIBundle.SwaggerUIStandalonePreset
        ],
    }})
    {oauth_init}
    </script>
</body>
</html>"""

    def _get_redoc_html(self) -> str:
        """Generate ReDoc HTML."""
        return f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{self.title} - ReDoc</title>
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <link rel="icon" type="image/png" href="https://fastapi.tiangolo.com/img/favicon.png" />
    <style>body {{ margin: 0; padding: 0; }}</style>
</head>
<body>
    <redoc spec-url="{self.openapi_url}"></redoc>
    <script src="https://cdn.jsdelivr.net/npm/redoc@2/bundles/redoc.standalone.js"></script>
</body>
</html>"""

    async def __call__(self, scope: Dict[str, Any], receive: Any, send: Any):
        """ASGI 3.0 interface implementation for ASGITransport, TestClient, uvicorn, and pytest."""
        import inspect, json
        if scope["type"] == "lifespan":
            while True:
                message = await receive()
                if message["type"] == "lifespan.startup":
                    for handler in getattr(self, "startup_handlers", []):
                        if inspect.iscoroutinefunction(handler):
                            await handler()
                        else:
                            handler()
                    await send({"type": "lifespan.startup.complete"})
                elif message["type"] == "lifespan.shutdown":
                    for handler in getattr(self, "shutdown_handlers", []):
                        if inspect.iscoroutinefunction(handler):
                            await handler()
                        else:
                            handler()
                    await send({"type": "lifespan.shutdown.complete"})
                    break
            return

        if scope["type"] != "http":
            return

        method = scope.get("method", "GET")
        path = scope.get("path", "/")
        query_string = scope.get("query_string", b"").decode("latin1")
        headers = {k.decode("latin1").lower(): v.decode("latin1") for k, v in scope.get("headers", [])}

        # Intercept OpenAPI/Swagger/ReDoc docs endpoints at Python level
        # to serve proper schemas with security schemes and full Swagger UI
        if method == "GET" and self._route_metadata:
            if path == self.openapi_url:
                openapi_spec = self.openapi()
                response_body = json.dumps(openapi_spec, ensure_ascii=False)
                resp_headers = {"content-type": "application/json; charset=utf-8"}
                # Drain the body
                while True:
                    msg = await receive()
                    if msg["type"] == "http.request" and not msg.get("more_body", False):
                        break
                encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
                await send({"type": "http.response.start", "status": 200, "headers": encoded_headers})
                await send({"type": "http.response.body", "body": response_body.encode("utf-8")})
                return

            if path == self.docs_url:
                response_body = self._get_swagger_ui_html()
                resp_headers = {"content-type": "text/html; charset=utf-8"}
                while True:
                    msg = await receive()
                    if msg["type"] == "http.request" and not msg.get("more_body", False):
                        break
                encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
                await send({"type": "http.response.start", "status": 200, "headers": encoded_headers})
                await send({"type": "http.response.body", "body": response_body.encode("utf-8")})
                return

            if self.redoc_url and path == self.redoc_url:
                response_body = self._get_redoc_html()
                resp_headers = {"content-type": "text/html; charset=utf-8"}
                while True:
                    msg = await receive()
                    if msg["type"] == "http.request" and not msg.get("more_body", False):
                        break
                encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
                await send({"type": "http.response.start", "status": 200, "headers": encoded_headers})
                await send({"type": "http.response.body", "body": response_body.encode("utf-8")})
                return

            if self.swagger_ui_oauth2_redirect_url and path == self.swagger_ui_oauth2_redirect_url:
                from .openapi.docs import get_swagger_ui_oauth2_redirect_html
                redirect_resp = get_swagger_ui_oauth2_redirect_html()
                response_body = getattr(redirect_resp, 'content', str(redirect_resp))
                resp_headers = {"content-type": "text/html; charset=utf-8"}
                while True:
                    msg = await receive()
                    if msg["type"] == "http.request" and not msg.get("more_body", False):
                        break
                encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
                await send({"type": "http.response.start", "status": 200, "headers": encoded_headers})
                await send({"type": "http.response.body", "body": response_body.encode("utf-8") if isinstance(response_body, str) else response_body})
                return

        body_bytes = bytearray()
        while True:
            msg = await receive()
            if msg["type"] == "http.request":
                body_bytes.extend(msg.get("body", b""))
                if not msg.get("more_body", False):
                    break

        body_str = body_bytes.decode("utf-8", errors="replace")

        try:
            status_code, response_body, resp_headers = await self.dispatch_request(
                method, path, query_string, headers, body_str
            )
        except Exception as exc:
            import logging
            logging.getLogger("rustapi").error(f"Error handling ASGI request [{method} {path}]: {exc}", exc_info=True)
            handler = self.exception_handlers.get(type(exc)) or self.exception_handlers.get(getattr(exc, "status_code", None))
            if handler:
                req = PyRequest(method=method, path=path, path_params={}, query_params={}, headers=headers, cookies={}, form={}, files={}, body=body_str)
                resp = await handler(req, exc) if inspect.iscoroutinefunction(handler) else handler(req, exc)
                status_code = getattr(resp, "status_code", 500)
                response_body = getattr(resp, "content", str(exc))
                resp_headers = getattr(resp, "headers", {"content-type": "application/json"})
            else:
                status_code = getattr(exc, "status_code", 500)
                detail = getattr(exc, "detail", str(exc))
                response_body = f'{{"detail": "{detail}"}}' if isinstance(detail, str) else json.dumps({"detail": detail})
                resp_headers = {"content-type": "application/json"}

        encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
        await send({
            "type": "http.response.start",
            "status": status_code,
            "headers": encoded_headers,
        })
        await send({
            "type": "http.response.body",
            "body": response_body.encode("utf-8") if isinstance(response_body, str) else response_body,
        })

    def exception_handler(self, exc_class_or_status_code: Any):
        """Register an exception handler decorator for an exception class or status code."""
        def decorator(func: Any):
            self.exception_handlers[exc_class_or_status_code] = func
            return func
        return decorator

    def frontend(self, path: str = "/", directory: str = "dist"):
        """Serve a built static frontend app (e.g. Vite, React, Vue, Svelte output directory)."""
        from .staticfiles import StaticFiles
        handler = StaticFiles(directory=directory, html=True)
        norm_path = path.rstrip("/")
        wildcard_path = f"{norm_path}/{{file_path:path}}" if norm_path else "/{file_path:path}"
        root_path = norm_path if norm_path else "/"

        self.get(root_path)(lambda: handler(""))
        self.get(wildcard_path)(lambda file_path="": handler(file_path))

Engine = FastAPI
Request = PyRequest

try:
    from ._rustapi import Response
except ImportError:
    from ._rustapi import PyResponse as Response

try:
    from ._rustapi import Database
except ImportError:
    pass


class HTMLResponse(Response):
    """HTML response wrapper automatically setting Content-Type: text/html; charset=utf-8."""

    def __new__(
        cls,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "text/html; charset=utf-8")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class JSONResponse(Response):
    """JSON response wrapper automatically setting Content-Type: application/json."""

    def __new__(
        cls,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "application/json")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class PlainTextResponse(Response):
    """Plain text response wrapper automatically setting Content-Type: text/plain; charset=utf-8."""

    def __new__(
        cls,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "text/plain; charset=utf-8")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class RedirectResponse(Response):
    """HTTP redirect response wrapper setting Location header."""

    def __new__(
        cls,
        url: str,
        status_code: int = 307,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h["Location"] = url
        return Response.__new__(cls, content="", status_code=status_code, headers=h)


__version__ = "1.8.9"
__all__ = [
    "Engine",
    "FastAPI",
    "Route",
    "PyRequest",
    "Request",
    "Response",
    "HTMLResponse",
    "JSONResponse",
    "PlainTextResponse",
    "RedirectResponse",
    "StreamingResponse",
    "FileResponse",
    "responses",
    "middleware",
    "security",
    "openapi",
    "EventSourceResponse",
    "ServerSentEvent",
    "format_sse_event",
    "HTTPException",
    "WebSocketException",
    "WebSocketDisconnect",
    "Depends",
    "APIRouter",
    "BackgroundTasks",
    "UploadFile",
    "WebSocket",
    "Database",
    "encode_jwt",
    "decode_jwt",
    "hash_password",
    "verify_password",
    "render_template",
    "status",
    "jsonable_encoder",
    "Body",
    "Cookie",
    "File",
    "Form",
    "Header",
    "Path",
    "Query",
    "Security",
]
