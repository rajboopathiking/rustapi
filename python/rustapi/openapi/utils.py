from typing import Any, Dict, List, Optional
from .constants import REF_PREFIX
from .models import OpenAPI, Info, Components, Server, Tag, SecurityScheme


def get_openapi(
    *,
    title: str,
    version: str,
    openapi_version: str = "3.1.0",
    description: Optional[str] = None,
    routes: Optional[List[Any]] = None,
    tags: Optional[List[Dict[str, Any]]] = None,
    servers: Optional[List[Dict[str, Any]]] = None,
    terms_of_service: Optional[str] = None,
    contact: Optional[Dict[str, Any]] = None,
    license_info: Optional[Dict[str, Any]] = None,
    separate_input_output_schemas: bool = True,
    security_schemes: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """Generate OpenAPI schema dictionary from routes and metadata."""
    info_data: Dict[str, Any] = {"title": title, "version": version}
    if description:
        info_data["description"] = description
    if terms_of_service:
        info_data["termsOfService"] = terms_of_service
    if contact:
        info_data["contact"] = contact
    if license_info:
        info_data["license"] = license_info

    output: Dict[str, Any] = {
        "openapi": openapi_version,
        "info": info_data,
        "paths": {},
    }

    if servers:
        output["servers"] = servers

    if tags:
        output["tags"] = tags

    components: Dict[str, Any] = {"schemas": {}, "securitySchemes": {}}

    if security_schemes:
        components["securitySchemes"].update(security_schemes)

    if routes:
        for route in routes:
            path = getattr(route, "path", None)
            methods = getattr(route, "methods", ["GET"])
            if not path:
                continue

            path_item = output["paths"].setdefault(path, {})

            for method in methods:
                method_lower = method.lower()
                if method_lower in ["options", "head"]:
                    continue

                operation_id = getattr(
                    route,
                    "operation_id",
                    f"{method_lower}_{path.replace('/', '_').strip('_')}",
                )

                operation: Dict[str, Any] = {
                    "summary": getattr(route, "summary", route.name if hasattr(route, "name") else operation_id),
                    "operationId": operation_id,
                    "responses": {
                        "200": {
                            "description": "Successful Response",
                            "content": {
                                "application/json": {
                                    "schema": {}
                                }
                            },
                        }
                    },
                }

                if hasattr(route, "description") and route.description:
                    operation["description"] = route.description
                if hasattr(route, "tags") and route.tags:
                    operation["tags"] = route.tags

                dependencies = getattr(route, "dependencies", [])
                route_security = []
                for dep in dependencies:
                    fn = getattr(dep, "func", None) or dep
                    scheme_name = getattr(fn, "scheme_name", None)
                    if scheme_name:
                        cls_name = fn.__class__.__name__
                        if cls_name == "HTTPBearer" or hasattr(fn, "bearerFormat"):
                            components["securitySchemes"][scheme_name] = {"type": "http", "scheme": "bearer"}
                        elif cls_name == "OAuth2PasswordBearer" or hasattr(fn, "tokenUrl"):
                            token_url = getattr(fn, "tokenUrl", "/token")
                            components["securitySchemes"][scheme_name] = {
                                "type": "oauth2",
                                "flows": {"password": {"tokenUrl": token_url, "scopes": {}}}
                            }
                        elif cls_name == "APIKeyHeader":
                            name = getattr(fn, "name", "X-API-Key")
                            components["securitySchemes"][scheme_name] = {"type": "apiKey", "name": name, "in": "header"}
                        elif cls_name == "APIKeyQuery":
                            name = getattr(fn, "name", "api_key")
                            components["securitySchemes"][scheme_name] = {"type": "apiKey", "name": name, "in": "query"}
                        elif cls_name == "APIKeyCookie":
                            name = getattr(fn, "name", "session")
                            components["securitySchemes"][scheme_name] = {"type": "apiKey", "name": name, "in": "cookie"}
                        elif cls_name == "HTTPBasic":
                            components["securitySchemes"][scheme_name] = {"type": "http", "scheme": "basic"}
                        elif cls_name == "HTTPDigest":
                            components["securitySchemes"][scheme_name] = {"type": "http", "scheme": "digest"}
                        elif cls_name == "OpenIdConnect":
                            openid_url = getattr(fn, "openIdConnectUrl", "")
                            components["securitySchemes"][scheme_name] = {"type": "openIdConnect", "openIdConnectUrl": openid_url}

                        if {scheme_name: []} not in route_security:
                            route_security.append({scheme_name: []})

                if route_security:
                    operation["security"] = route_security

                path_item[method_lower] = operation

    if components["schemas"] or components["securitySchemes"]:
        output["components"] = {k: v for k, v in components.items() if v}

    return output
