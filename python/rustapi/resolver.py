import inspect
import asyncio
from typing import Any, Dict, Optional
from .depends import Depends
from .exceptions import HTTPException


async def solve_dependency(
    dep_target: Any,
    request: Any,
    dependency_overrides: Optional[Dict[Any, Any]] = None,
    cache: Optional[Dict[Any, Any]] = None,
    teardown_list: Optional[list] = None,
    scope: str = "function",
) -> Any:
    """Recursively resolve FastAPI dependencies with request injection, generator support, and caching."""
    if cache is None:
        cache = {}

    if dependency_overrides and dep_target in dependency_overrides:
        dep_target = dependency_overrides[dep_target]

    if isinstance(dep_target, Depends):
        scope = getattr(dep_target, "scope", None) or scope or "function"
        dep_target = dep_target.dependency

    if not callable(dep_target):
        return dep_target

    target_id = id(dep_target)
    if target_id in cache:
        return cache[target_id]

    try:
        sig = inspect.signature(dep_target)
        kwargs = {}
        for param_name, param in sig.parameters.items():
            if param.kind in (inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.VAR_KEYWORD):
                continue

            # 1. Inject request if parameter expects Request
            if param_name in ("request", "req") or (param.annotation and getattr(param.annotation, "__name__", "") in ("PyRequest", "Request")):
                kwargs[param_name] = request
                continue

            # 2. Check if default value is Depends(...)
            default_val = param.default
            if isinstance(default_val, Depends):
                sub_scope = getattr(default_val, "scope", None) or "function"
                sub_target = default_val.dependency or (param.annotation if param.annotation != inspect.Parameter.empty else None)
                kwargs[param_name] = await solve_dependency(sub_target, request, dependency_overrides, cache, teardown_list, scope=sub_scope)
                continue

            # 3. Check Annotated metadata or type hints
            annotated_args = getattr(param.annotation, "__metadata__", ())
            for arg in annotated_args:
                if isinstance(arg, Depends):
                    sub_scope = getattr(arg, "scope", None) or "function"
                    sub_target = arg.dependency or param.annotation
                    kwargs[param_name] = await solve_dependency(sub_target, request, dependency_overrides, cache, teardown_list, scope=sub_scope)
                    break

    except (ValueError, TypeError):
        sig = None
        kwargs = {}

    if not kwargs and not sig:
        try:
            res = dep_target(request) if callable(dep_target) else dep_target
        except TypeError:
            res = dep_target()
    else:
        if inspect.isgeneratorfunction(dep_target) or inspect.isasyncgenfunction(dep_target):
            gen = dep_target(**kwargs)
            if inspect.isasyncgenfunction(dep_target):
                res = await gen.__anext__()
            else:
                res = next(gen)
            if teardown_list is not None:
                teardown_list.append((scope, gen))
        elif asyncio.iscoroutinefunction(dep_target) or (hasattr(dep_target, "__call__") and asyncio.iscoroutinefunction(dep_target.__call__)):
            res = await dep_target(**kwargs)
        else:
            res = dep_target(**kwargs)

    cache[target_id] = res
    return res


async def teardown_dependencies(teardown_list: list, target_scope: Optional[str] = None):
    """Teardown generator dependencies in strict reverse (LIFO) order.
    
    If target_scope is provided ('function' or 'request'), only matching dependencies
    are popped and advanced, leaving other scopes intact for later phases.
    If target_scope is None, all remaining dependencies are popped and cleaned up.
    """
    if not teardown_list:
        return

    i = len(teardown_list) - 1
    while i >= 0:
        if i >= len(teardown_list):
            i = len(teardown_list) - 1
            if i < 0:
                break
        item = teardown_list[i]
        if isinstance(item, tuple) and len(item) == 2:
            scope, gen = item
        else:
            scope, gen = "function", item

        if target_scope is None or scope == target_scope:
            teardown_list.pop(i)
            if inspect.isasyncgen(gen):
                try:
                    await gen.__anext__()
                except (StopAsyncIteration, GeneratorExit):
                    pass
                except Exception:
                    pass
                finally:
                    try:
                        await gen.aclose()
                    except Exception:
                        pass
            elif inspect.isgenerator(gen):
                try:
                    next(gen)
                except (StopIteration, GeneratorExit):
                    pass
                except Exception:
                    pass
                finally:
                    try:
                        gen.close()
                    except Exception:
                        pass
        i -= 1
