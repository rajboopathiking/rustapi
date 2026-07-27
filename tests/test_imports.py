import os
import subprocess
import sys
from pathlib import Path


def test_public_package_exports_work():
    import rustapi

    assert rustapi.Engine is not None
    assert rustapi.Response is not None
    assert rustapi.HTTPException is not None
    assert rustapi.Depends is not None

    response = rustapi.Response({"ok": True}, status_code=201)
    assert response.status_code == 201
    assert response.content == {"ok": True}


def test_repo_root_import_prefers_workspace_package():
    repo_root = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)

    result = subprocess.run(
        [
            sys.executable,
            "-c",
            "import rustapi; from rustapi import APIRouter; print(rustapi.__file__)",
        ],
        cwd=repo_root,
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert str(repo_root / "python" / "rustapi" / "__init__.py") in result.stdout
