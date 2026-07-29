# 🤖 Embedded Model Context Protocol (MCP) Server

RustAPI includes a built-in Model Context Protocol (MCP) server running on the same HTTP instance at `POST /mcp`.

This enables AI Agents (e.g., Claude Desktop, Antigravity, custom agents) to inspect tools, prompts, and resources exposed by your application.

---

## 🛠️ Registering Tools (`@app.tool`)

Expose Python functions as MCP tools:

```python
from rustapi import Engine

app = Engine()

@app.tool(name="calculate_tax", description="Calculates tax for a total amount")
def calculate_tax(amount: float, rate: float = 0.05) -> float:
    return amount * rate
```

---

## 📁 Registering Resources (`@app.resource`)

Expose static or dynamic resources:

```python
@app.resource("config://app-settings", mime_type="application/json")
def get_config():
    return {"environment": "production", "version": "0.1.19"}
```

---

## 💬 Registering Prompts (`@app.prompt`)

Provide prompt templates for AI workflows:

```python
@app.prompt(name="code_review", description="Generate code review prompt")
def code_review_prompt(language: str):
    return f"Please review the following {language} code snippet for security vulnerabilities."
```

---

## 📡 MCP Endpoint Protocol

The MCP endpoint operates at `POST /mcp` over JSON-RPC 2.0. Supported methods:
* `initialize`
* `tools/list`
* `tools/call`
* `resources/list`
* `resources/read`
* `prompts/list`
* `prompts/get`
* `ping`
