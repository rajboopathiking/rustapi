# RustAPI

A FastAPI-style Python web framework backed by a Rust (`tokio` + `hyper`) core.
Python gives you the developer experience; Rust owns the socket, async I/O,
routing, and parameter parsing.

```A FastAPI-style framework that minimizes Python overhead by moving networking, routing, and request parsing into Rust. It's designed to outperform traditional Python web frameworks on framework overhead while preserving Python's developer experience.```

```python
import rustapi

app = rustapi.Engine()


@app.get("/hello")
def hello():
    return {"message": "hello from a Python function, routed by Rust"}


@app.get("/users/{user_id}")
def get_user(request):
    return {"user_id": request.path_params["user_id"]}


app.run(host="127.0.0.1", port=8000)
```

## Status: early alpha

This is a working core, not a feature-complete FastAPI replacement yet.
See [CHANGELOG.md](CHANGELOG.md) for release history.

## Why

Rust-core Python libraries (Polars over Pandas, Ruff over older linters,
Granian over Uvicorn) consistently outperform their pure-Python equivalents
by moving I/O, parsing, and routing into compiled code while keeping
Python for business logic. RustAPI applies that same pattern to web
routing specifically.

## What's implemented

- Async I/O core on `tokio` + `hyper` — not thread-per-connection.
- Decorator-based routing: `@app.get`, `.post`, `.put`, `.delete`.
- Path parameters (`/users/{user_id}`) and query parameters, parsed and
  URL-decoded in Rust.
- Request body available via `request.body`.
- GIL released for the server's full lifetime; re-acquired per-request
  only for the duration of your Python handler call.
- Handler arity detection — a 0-arg handler is called with no arguments;
  add a `request` parameter to receive path/query params and body.

## Install

```bash
pip install rustapi
```

(Not yet published — see [Publishing](#publishing-to-pypi) below if you're
building this from source.)

## Develop locally

Requires the Rust toolchain (`rustc`, `cargo`) and `maturin`.

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install maturin pytest requests
maturin develop --release
python examples/app.py
```

## Test

```bash
pytest tests/ -v
```

The test suite starts a real server on a background thread and exercises
it over actual HTTP — routing, path params, query params, POST bodies,
404s, and 30 concurrent requests.

## Benchmark

One `ab -n 5000 -c 50` run, same machine, back-to-back against FastAPI +
uvicorn (single worker):

| | req/s | mean latency |
|---|---|---|
| FastAPI + uvicorn (1 worker) | 2,210 | 22.6ms |
| RustAPI | 4,651 | 10.7ms |

Read this as "the architecture works," not "beats Java" — see
[ROADMAP.md](ROADMAP.md) Phase 8–9 for what's actually required to make
that claim honestly.

## Publishing to PyPI

This repo ships a GitHub Actions workflow (`.github/workflows/CI.yml`) that:
1. Runs the test suite on Linux/macOS/Windows on every push and PR.
2. On a `v*` tag push, builds release wheels for all three platforms via
   `maturin-action`.
3. Publishes to PyPI using trusted publishing.

To cut a release: bump `version` in both `Cargo.toml` and `pyproject.toml`,
commit, tag (`git tag v0.1.0 && git push --tags`), and the workflow does
the rest.

## License

MIT — see [LICENSE](LICENSE).

## Contributing

Early-stage project — issues and PRs welcome. See ROADMAP.md for the
prioritized list of what's next.