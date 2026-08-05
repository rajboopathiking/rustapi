import os
from typing import Optional
from ._rustapi import Response


class StaticFiles:
    """Static file serving handler for SPA build output directories."""

    def __init__(self, directory: str, html: bool = True):
        self.directory = directory
        self.html = html

    def get_content_type(self, filepath: str) -> str:
        if filepath.endswith(".html"): return "text/html; charset=utf-8"
        if filepath.endswith(".css"): return "text/css"
        if filepath.endswith(".js"): return "application/javascript"
        if filepath.endswith(".json"): return "application/json"
        if filepath.endswith(".png"): return "image/png"
        if filepath.endswith(".jpg") or filepath.endswith(".jpeg"): return "image/jpeg"
        if filepath.endswith(".svg"): return "image/svg+xml"
        if filepath.endswith(".ico"): return "image/x-icon"
        if filepath.endswith(".wasm"): return "application/wasm"
        return "application/octet-stream"

    def __call__(self, file_path: str = "") -> Response:
        target = os.path.join(self.directory, file_path) if file_path else self.directory
        if os.path.isdir(target):
            target = os.path.join(target, "index.html")

        if os.path.isfile(target):
            with open(target, "rb") as f:
                content = f.read()
            return Response(content, status_code=200, headers={"Content-Type": self.get_content_type(target)})

        index_target = os.path.join(self.directory, "index.html")
        if self.html and os.path.isfile(index_target):
            with open(index_target, "rb") as f:
                content = f.read()
            return Response(content, status_code=200, headers={"Content-Type": "text/html; charset=utf-8"})

        return Response("Not Found", status_code=404)
