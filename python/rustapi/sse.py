import json
from typing import Any, Dict, Optional
from pydantic import BaseModel, Field, model_validator
from ._rustapi import StreamingResponse


class EventSourceResponse:
    """Streaming response with `text/event-stream` media type for Server-Sent Events."""

    media_type = "text/event-stream"

    def __new__(
        cls,
        content: Any,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
        media_type: Optional[str] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "text/event-stream")
        actual_media_type = media_type or cls.media_type
        res = StreamingResponse(
            content=content,
            status_code=status_code,
            headers=h,
            media_type=actual_media_type,
        )
        try:
            setattr(res, "media_type", actual_media_type)
        except AttributeError:
            pass
        return res


class ServerSentEvent(BaseModel):
    """Represents a single Server-Sent Event."""

    data: Optional[Any] = None
    raw_data: Optional[str] = None
    event: Optional[str] = None
    id: Optional[str] = None
    retry: Optional[int] = Field(default=None, ge=0)
    comment: Optional[str] = None

    @model_validator(mode="after")
    def _check_data_exclusive(self) -> "ServerSentEvent":
        if self.data is not None and self.raw_data is not None:
            raise ValueError(
                "Cannot set both 'data' and 'raw_data' on the same ServerSentEvent."
            )
        return self


def _split_sse_lines(value: str) -> list[str]:
    return value.replace("\r\n", "\n").replace("\r", "\n").split("\n")


def format_sse_event(
    *,
    data_str: Optional[str] = None,
    event: Optional[str] = None,
    id: Optional[str] = None,
    retry: Optional[int] = None,
    comment: Optional[str] = None,
) -> bytes:
    """Build SSE wire-format bytes from event parameters.

    The result always ends with `\\n\\n` (the event terminator).
    """
    lines: list[str] = []

    if comment is not None:
        for line in _split_sse_lines(comment):
            lines.append(f": {line}")

    if event is not None:
        lines.append(f"event: {event}")

    if data_str is not None:
        for line in _split_sse_lines(data_str):
            lines.append(f"data: {line}")

    if id is not None:
        lines.append(f"id: {id}")

    if retry is not None:
        lines.append(f"retry: {retry}")

    lines.append("")
    lines.append("")
    return "\n".join(lines).encode("utf-8")


KEEPALIVE_COMMENT = b": ping\n\n"
_PING_INTERVAL: float = 15.0
