import io
from typing import Optional, Dict, Any


class AwaitableDict(dict):
    """Dict subclass that is directly awaitable for async compatibility (await req.json())."""
    def __await__(self):
        async def _wrap():
            return self
        return _wrap().__await__()


class AwaitableBytes(bytes):
    """Bytes subclass that is directly awaitable for async compatibility (await file.read())."""
    def __await__(self):
        async def _wrap():
            return self
        return _wrap().__await__()


class AwaitableInt(int):
    """Integer subclass that is directly awaitable for async compatibility (await file.seek())."""
    def __await__(self):
        async def _wrap():
            return self
        return _wrap().__await__()


class AwaitableNone:
    """None wrapper that is directly awaitable for async compatibility (await file.close())."""
    def __await__(self):
        async def _wrap():
            return None
        return _wrap().__await__()


class UploadFile:
    """FastAPI-compatible UploadFile supporting both async (await file.read()) and sync access."""

    def __init__(
        self,
        file: Optional[io.BytesIO] = None,
        filename: Optional[str] = "",
        content_type: str = "",
        headers: Optional[Dict[str, str]] = None,
    ):
        self.file = file or io.BytesIO()
        self.filename = filename or ""
        self.content_type = content_type or ""
        self.headers = headers or {}

    def read(self, size: int = -1) -> AwaitableBytes:
        content = self.file.read(size)
        return AwaitableBytes(content)

    def seek(self, offset: int = 0) -> AwaitableInt:
        res = self.file.seek(offset)
        return AwaitableInt(res)

    def write(self, data: bytes) -> AwaitableInt:
        res = self.file.write(data)
        return AwaitableInt(res)

    def save_to(self, destination_path: str) -> AwaitableInt:
        """Stream file contents directly to destination file path."""
        self.file.seek(0)
        with open(destination_path, "wb") as f:
            written = f.write(self.file.read())
        return AwaitableInt(written)

    @property
    def size(self) -> int:
        """Total size of uploaded file in bytes."""
        pos = self.file.tell()
        self.file.seek(0, io.SEEK_END)
        s = self.file.tell()
        self.file.seek(pos)
        return s

    def close(self) -> AwaitableNone:
        self.file.close()
        return AwaitableNone()
