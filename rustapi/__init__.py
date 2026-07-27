from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

repo_root = Path(__file__).resolve().parent.parent
python_package_dir = repo_root / "python" / "rustapi"

if not python_package_dir.exists():
    raise ImportError(f"Workspace package directory not found: {python_package_dir}")

__path__ = [str(python_package_dir)] + [str(p) for p in __path__]

_impl_name = "_rustapi_workspace_impl"
if _impl_name not in sys.modules:
    spec = importlib.util.spec_from_file_location(
        _impl_name,
        python_package_dir / "__init__.py",
        submodule_search_locations=[str(python_package_dir)],
    )
    if spec is None or spec.loader is None:
        raise ImportError(f"Unable to load workspace package from {python_package_dir}")
    _impl_module = importlib.util.module_from_spec(spec)
    sys.modules[_impl_name] = _impl_module
    spec.loader.exec_module(_impl_module)
else:
    _impl_module = sys.modules[_impl_name]

for _name in getattr(_impl_module, "__all__", []):
    globals()[_name] = getattr(_impl_module, _name)

__version__ = getattr(_impl_module, "__version__", "0.1.17")
__all__ = list(getattr(_impl_module, "__all__", []))
