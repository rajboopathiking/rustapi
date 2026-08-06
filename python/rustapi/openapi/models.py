from typing import Any, Dict, List, Literal, Optional, Union
from pydantic import BaseModel, Field


class BaseModelWithConfig(BaseModel):
    model_config = {"extra": "allow"}


class Contact(BaseModelWithConfig):
    name: Optional[str] = None
    url: Optional[str] = None
    email: Optional[str] = None


class License(BaseModelWithConfig):
    name: str
    identifier: Optional[str] = None
    url: Optional[str] = None


class Info(BaseModelWithConfig):
    title: str
    summary: Optional[str] = None
    description: Optional[str] = None
    termsOfService: Optional[str] = None
    contact: Optional[Contact] = None
    license: Optional[License] = None
    version: str


class ServerVariable(BaseModelWithConfig):
    enum: Optional[List[str]] = None
    default: str
    description: Optional[str] = None


class Server(BaseModelWithConfig):
    url: str
    description: Optional[str] = None
    variables: Optional[Dict[str, ServerVariable]] = None


class Reference(BaseModel):
    ref: str = Field(alias="$ref")


class Discriminator(BaseModel):
    propertyName: str
    mapping: Optional[Dict[str, str]] = None


class XML(BaseModelWithConfig):
    name: Optional[str] = None
    namespace: Optional[str] = None
    prefix: Optional[str] = None
    attribute: Optional[bool] = None
    wrapped: Optional[bool] = None


class ExternalDocumentation(BaseModelWithConfig):
    description: Optional[str] = None
    url: str


class ParameterInType(str):
    query = "query"
    header = "header"
    path = "path"
    cookie = "cookie"


class Parameter(BaseModelWithConfig):
    name: str
    param_in: str = Field(alias="in")
    description: Optional[str] = None
    required: Optional[bool] = None
    deprecated: Optional[bool] = None
    allowEmptyValue: Optional[bool] = None
    style: Optional[str] = None
    explode: Optional[bool] = None
    schema_: Optional[Union[Reference, Dict[str, Any]]] = Field(default=None, alias="schema")


class MediaType(BaseModelWithConfig):
    schema_: Optional[Union[Reference, Dict[str, Any]]] = Field(default=None, alias="schema")
    example: Optional[Any] = None
    examples: Optional[Dict[str, Any]] = None


class Response(BaseModelWithConfig):
    description: str
    headers: Optional[Dict[str, Any]] = None
    content: Optional[Dict[str, MediaType]] = None


class RequestBody(BaseModelWithConfig):
    description: Optional[str] = None
    content: Dict[str, MediaType]
    required: Optional[bool] = None


class Operation(BaseModelWithConfig):
    tags: Optional[List[str]] = None
    summary: Optional[str] = None
    description: Optional[str] = None
    externalDocs: Optional[ExternalDocumentation] = None
    operationId: Optional[str] = None
    parameters: Optional[List[Union[Parameter, Reference]]] = None
    requestBody: Optional[Union[RequestBody, Reference]] = None
    responses: Optional[Dict[str, Union[Response, Reference]]] = None
    deprecated: Optional[bool] = None
    security: Optional[List[Dict[str, List[str]]]] = None
    servers: Optional[List[Server]] = None


class PathItem(BaseModelWithConfig):
    ref: Optional[str] = Field(default=None, alias="$ref")
    summary: Optional[str] = None
    description: Optional[str] = None
    get: Optional[Operation] = None
    put: Optional[Operation] = None
    post: Optional[Operation] = None
    delete: Optional[Operation] = None
    options: Optional[Operation] = None
    head: Optional[Operation] = None
    patch: Optional[Operation] = None
    trace: Optional[Operation] = None
    servers: Optional[List[Server]] = None
    parameters: Optional[List[Union[Parameter, Reference]]] = None


class APIKeyIn(str):
    query = "query"
    header = "header"
    cookie = "cookie"


class APIKey(BaseModelWithConfig):
    type_: str = Field("apiKey", alias="type")
    description: Optional[str] = None
    name: str
    in_: str = Field(alias="in")


class HTTPBase(BaseModelWithConfig):
    type_: str = Field("http", alias="type")
    description: Optional[str] = None
    scheme: str


class HTTPBearer(HTTPBase):
    scheme: str = "bearer"
    bearerFormat: Optional[str] = None


class OAuthFlow(BaseModelWithConfig):
    refreshUrl: Optional[str] = None
    scopes: Dict[str, str] = {}


class OAuthFlowImplicit(OAuthFlow):
    authorizationUrl: str


class OAuthFlowPassword(OAuthFlow):
    tokenUrl: str


class OAuthFlowClientCredentials(OAuthFlow):
    tokenUrl: str


class OAuthFlowAuthorizationCode(OAuthFlow):
    authorizationUrl: str
    tokenUrl: str


class OAuthFlows(BaseModelWithConfig):
    implicit: Optional[OAuthFlowImplicit] = None
    password: Optional[OAuthFlowPassword] = None
    clientCredentials: Optional[OAuthFlowClientCredentials] = None
    authorizationCode: Optional[OAuthFlowAuthorizationCode] = None


class OAuth2(BaseModelWithConfig):
    type_: str = Field("oauth2", alias="type")
    description: Optional[str] = None
    flows: OAuthFlows


class OpenIdConnect(BaseModelWithConfig):
    type_: str = Field("openIdConnect", alias="type")
    description: Optional[str] = None
    openIdConnectUrl: str


SecurityScheme = Union[APIKey, HTTPBase, HTTPBearer, OAuth2, OpenIdConnect]


class Components(BaseModelWithConfig):
    schemas: Optional[Dict[str, Any]] = None
    responses: Optional[Dict[str, Any]] = None
    parameters: Optional[Dict[str, Any]] = None
    requestBodies: Optional[Dict[str, Any]] = None
    securitySchemes: Optional[Dict[str, SecurityScheme]] = None


class Tag(BaseModelWithConfig):
    name: str
    description: Optional[str] = None
    externalDocs: Optional[ExternalDocumentation] = None


class OpenAPI(BaseModelWithConfig):
    openapi: str = "3.1.0"
    info: Info
    servers: Optional[List[Server]] = None
    paths: Dict[str, PathItem] = {}
    components: Optional[Components] = None
    security: Optional[List[Dict[str, List[str]]]] = None
    tags: Optional[List[Tag]] = None
    externalDocs: Optional[ExternalDocumentation] = None
