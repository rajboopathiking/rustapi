from enum import Enum
from pathlib import Path
from types import GeneratorType
from typing import Any, Callable, Dict, List, Optional, Set, Tuple, Union
from dataclasses import is_dataclass, asdict
import datetime
import decimal
import uuid

try:
    from pydantic import BaseModel
except ImportError:
    BaseModel = None


def jsonable_encoder(
    obj: Any,
    include: Optional[Union[Set[Union[int, str]], Dict[Union[int, str], Any]]] = None,
    exclude: Optional[Union[Set[Union[int, str]], Dict[Union[int, str], Any]]] = None,
    by_alias: bool = True,
    exclude_unset: bool = False,
    exclude_defaults: bool = False,
    exclude_none: bool = False,
    custom_encoder: Optional[Dict[Any, Callable[[Any], Any]]] = None,
    sqlalchemy_safe: bool = True,
) -> Any:
    """Convert any Python object to a JSON-serializable structure (dict, list, int, float, str, etc.)."""
    custom_encoder = custom_encoder or {}
    if custom_encoder:
        if type(obj) in custom_encoder:
            return custom_encoder[type(obj)](obj)

    if BaseModel is not None and isinstance(obj, BaseModel):
        # Handle Pydantic v1 / v2 model serialization
        if hasattr(obj, "model_dump"):
            encoder_dict = obj.model_dump(
                mode="json",
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )
        else:
            encoder_dict = obj.dict(
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )

        if include or exclude:
            # apply basic key filtering
            keys = set(encoder_dict.keys())
            if include:
                keys = keys.intersection(set(include))
            if exclude:
                keys = keys.difference(set(exclude))
            encoder_dict = {k: encoder_dict[k] for k in keys if k in encoder_dict}

        return jsonable_encoder(
            encoder_dict,
            by_alias=by_alias,
            exclude_unset=exclude_unset,
            exclude_defaults=exclude_defaults,
            exclude_none=exclude_none,
            custom_encoder=custom_encoder,
            sqlalchemy_safe=sqlalchemy_safe,
        )

    if is_dataclass(obj) and not isinstance(obj, type):
        return jsonable_encoder(
            asdict(obj),
            include=include,
            exclude=exclude,
            by_alias=by_alias,
            exclude_unset=exclude_unset,
            exclude_defaults=exclude_defaults,
            exclude_none=exclude_none,
            custom_encoder=custom_encoder,
            sqlalchemy_safe=sqlalchemy_safe,
        )

    if isinstance(obj, Enum):
        return obj.value

    if isinstance(obj, (str, int, float, bool, type(None))):
        return obj

    if isinstance(obj, (datetime.date, datetime.datetime, datetime.time)):
        return obj.isoformat()

    if isinstance(obj, (uuid.UUID, decimal.Decimal, Path)):
        return str(obj)

    if isinstance(obj, (list, set, tuple, GeneratorType)):
        encoded_list = []
        for item in obj:
            encoded_list.append(
                jsonable_encoder(
                    item,
                    include=include,
                    exclude=exclude,
                    by_alias=by_alias,
                    exclude_unset=exclude_unset,
                    exclude_defaults=exclude_defaults,
                    exclude_none=exclude_none,
                    custom_encoder=custom_encoder,
                    sqlalchemy_safe=sqlalchemy_safe,
                )
            )
        return encoded_list

    if isinstance(obj, dict):
        encoded_dict = {}
        for key, value in obj.items():
            if exclude_none and value is None:
                continue
            if include and key not in include:
                continue
            if exclude and key in exclude:
                continue
            encoded_key = jsonable_encoder(
                key,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
                custom_encoder=custom_encoder,
                sqlalchemy_safe=sqlalchemy_safe,
            )
            encoded_value = jsonable_encoder(
                value,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
                custom_encoder=custom_encoder,
                sqlalchemy_safe=sqlalchemy_safe,
            )
            encoded_dict[encoded_key] = encoded_value
        return encoded_dict

    # Fallback to dict conversion if available
    if hasattr(obj, "__dict__"):
        return jsonable_encoder(
            obj.__dict__,
            include=include,
            exclude=exclude,
            by_alias=by_alias,
            exclude_unset=exclude_unset,
            exclude_defaults=exclude_defaults,
            exclude_none=exclude_none,
            custom_encoder=custom_encoder,
            sqlalchemy_safe=sqlalchemy_safe,
        )

    return str(obj)


def filter_response_data(
    val: Any,
    response_model: Any,
    *,
    include: Optional[Any] = None,
    exclude: Optional[Any] = None,
    by_alias: bool = True,
    exclude_unset: bool = False,
    exclude_defaults: bool = False,
    exclude_none: bool = False,
) -> Any:
    """Filter, validate, and serialize endpoint return values against a Pydantic response_model."""
    if val is None or response_model is None:
        return val

    from typing import get_origin, get_args

    def _dump_single(model_cls: Any, item: Any) -> Any:
        if BaseModel is not None:
            # If item is already a validated BaseModel
            if isinstance(item, BaseModel):
                if hasattr(item, "model_dump"):
                    return item.model_dump(
                        mode="json",
                        include=include,
                        exclude=exclude,
                        by_alias=by_alias,
                        exclude_unset=exclude_unset,
                        exclude_defaults=exclude_defaults,
                        exclude_none=exclude_none,
                    )
                else:
                    return item.dict(
                        include=include,
                        exclude=exclude,
                        by_alias=by_alias,
                        exclude_unset=exclude_unset,
                        exclude_defaults=exclude_defaults,
                        exclude_none=exclude_none,
                    )

            # If model_cls is a BaseModel subclass
            if isinstance(model_cls, type) and issubclass(model_cls, BaseModel):
                if hasattr(model_cls, "model_validate"):
                    validated = model_cls.model_validate(item, from_attributes=True)
                    return validated.model_dump(
                        mode="json",
                        include=include,
                        exclude=exclude,
                        by_alias=by_alias,
                        exclude_unset=exclude_unset,
                        exclude_defaults=exclude_defaults,
                        exclude_none=exclude_none,
                    )
                elif hasattr(model_cls, "parse_obj"):
                    validated = model_cls.parse_obj(item)
                    return validated.dict(
                        include=include,
                        exclude=exclude,
                        by_alias=by_alias,
                        exclude_unset=exclude_unset,
                        exclude_defaults=exclude_defaults,
                        exclude_none=exclude_none,
                    )

        try:
            from pydantic import TypeAdapter
            adapter = TypeAdapter(model_cls)
            validated = adapter.validate_python(item, from_attributes=True)
            return adapter.dump_python(
                validated,
                mode="json",
                include=include,
                exclude=exclude,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )
        except Exception:
            return jsonable_encoder(
                item,
                include=include,
                exclude=exclude,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )

    origin = get_origin(response_model)
    args = get_args(response_model)
    if origin in (list, tuple, set) and args and isinstance(val, (list, tuple, set)):
        elem_cls = args[0]
        return [_dump_single(elem_cls, item) for item in val]

    if isinstance(val, list) and isinstance(response_model, type) and issubclass(response_model, BaseModel):
        return [_dump_single(response_model, item) for item in val]

    return _dump_single(response_model, val)


def wrap_handler_with_response_model(func: Any, response_model: Any, **kwargs: Any) -> Any:
    """Wrap a sync or async endpoint handler with Pydantic response filtering."""
    if not response_model:
        return func

    import functools
    import inspect

    include = kwargs.get("response_model_include")
    exclude = kwargs.get("response_model_exclude")
    by_alias = kwargs.get("response_model_by_alias", True)
    exclude_unset = kwargs.get("response_model_exclude_unset", False)
    exclude_defaults = kwargs.get("response_model_exclude_defaults", False)
    exclude_none = kwargs.get("response_model_exclude_none", False)

    if inspect.iscoroutinefunction(func):
        @functools.wraps(func)
        async def async_response_wrapper(*args: Any, **kw: Any) -> Any:
            raw_res = await func(*args, **kw)
            return filter_response_data(
                raw_res,
                response_model,
                include=include,
                exclude=exclude,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )
        return async_response_wrapper
    else:
        @functools.wraps(func)
        def sync_response_wrapper(*args: Any, **kw: Any) -> Any:
            raw_res = func(*args, **kw)
            return filter_response_data(
                raw_res,
                response_model,
                include=include,
                exclude=exclude,
                by_alias=by_alias,
                exclude_unset=exclude_unset,
                exclude_defaults=exclude_defaults,
                exclude_none=exclude_none,
            )
        return sync_response_wrapper

