# 🗄️ Rust-Native Database Engine

RustAPI includes an embedded `sqlx` database pool for PostgreSQL and SQLite.

By executing SQL queries in Rust and streaming UTF-8 JSON bytes directly to the TCP socket, RustAPI completely bypasses the Python GIL and Pydantic object allocation overhead.

---

## 🔌 Connecting to Database

```python
from rustapi import Engine

app = Engine()

# SQLite Memory Database
db = app.connect_db("sqlite::memory:")

# PostgreSQL Connection Pool
# db = app.connect_db("postgres://postgres:password@localhost/mydb")
```

---

## ⚡ Executing DDL / DML (`db.execute`)

Use `db.execute()` to run schema migrations, insertions, updates, and deletions:

```python
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
rows_affected = db.execute("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')")
```

---

## 🚀 Zero-Copy JSON Streaming (`db.query_json`)

Return `db.query_json(sql)` directly from handler functions to stream database results to HTTP clients with maximum performance:

```python
@app.get("/users")
def list_users():
    # Executes query in Rust and streams JSON bytes directly to socket
    return db.query_json("SELECT id, name, email FROM users")
```
