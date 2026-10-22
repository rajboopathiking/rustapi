import ast
import os
import pytest


def test_no_duplicate_methods_in_classes():
    """Verify via AST parsing that no class defines duplicate method names."""
    pkg_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "python", "rustapi"))
    duplicates = []

    for root, _, files in os.walk(pkg_dir):
        for f in files:
            if f.endswith(".py"):
                path = os.path.join(root, f)
                with open(path, "r", encoding="utf-8") as py_file:
                    content = py_file.read()
                tree = ast.parse(content, filename=path)
                for node in ast.walk(tree):
                    if isinstance(node, ast.ClassDef):
                        seen = set()
                        for item in node.body:
                            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                                if item.name in seen:
                                    duplicates.append((os.path.basename(path), node.name, item.name, item.lineno))
                                seen.add(item.name)

    assert not duplicates, f"Found duplicate class method definitions: {duplicates}"
