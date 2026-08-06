# ⚡ Embedded Rust Power Primitives

RustAPI exposes native, C-speed security and templating functions directly in Python.

---

## 🔑 Native Rust JWT Engine (`jsonwebtoken`)

Perform JWT encoding and decoding natively in Rust without `pyjwt` latency:

```python
from rustapi import encode_jwt, decode_jwt

# Encode JWT (Supports HS256, HS384, HS512)
payload = {"sub": "user_42", "role": "admin"}
token = encode_jwt(payload, secret="super_secret_key", algorithm="HS256")

# Decode JWT
claims = decode_jwt(token, secret="super_secret_key", algorithm="HS256")
print(claims["sub"])  # Output: user_42
```

---

## 🔒 High-Speed Argon2 Password Hashing (`argon2`)

Hash and verify passwords safely without blocking Python threads. Tokio blocking worker pools execute Argon2 hashing in background C threads while releasing the Python GIL:

```python
from rustapi import hash_password, verify_password

# Hash password
pw_hash = hash_password("MySecurePassword123!")
print(pw_hash)  # Output: $argon2id$v=19$m=19456...

# Verify password
is_valid = verify_password("MySecurePassword123!", pw_hash)
assert is_valid is True
```

---

## 🎨 Native MiniJinja Template Renderer (`minijinja`)

Render Jinja2 templates directly in Rust memory:

```python
from rustapi import render_template, HTMLResponse

template = "<h1>Welcome {{ name }}!</h1><p>Active items: {{ items | length }}</p>"
context = {"name": "Boopathi", "items": ["Item A", "Item B"]}

rendered_html = render_template(template, context)
response = HTMLResponse(rendered_html)
```
