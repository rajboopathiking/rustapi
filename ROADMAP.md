# PyRustAPI Engineering Roadmap

This document tracks the verified state of RustAPI and the phased implementation plan to reach feature-parity with production Python frameworks (like FastAPI) without sacrificing Rust's native speed.

## §0. Ground Truth (Already Shipped & Verified)
*Before adding new features, verify they aren't already implemented.*
- [x] **Hyper 0.14 HTTP Engine:** Core TCP socket binding and request parsing.
- [x] **Radix Tree Routing:** High-speed URL matching in Rust.
- [x] **Sync Route Offloading:** `def` routes correctly execute inside Tokio's `spawn_blocking` pool.
- [x] **Basic CORS Support:** Middleware for handling cross-origin requests.
- [x] **HTTP Method Support:** `GET`, `POST`, `OPTIONS`, `HEAD`, `PATCH` routing.
- [x] **Basic OpenAPI / Docs:** Foundational `/docs` and `/openapi.json` generation.
- [x] **Concurrency Thread Thrashing Fix:** GIL Semaphore restricts OS thread limits under extreme load.

---

## §1. Phase 1: HTTP Metadata & Error Handling (Size: M) [COMPLETED]
*Goal: Give handlers full control over HTTP requests and structured error responses.*
- [x] **Request Headers & Cookies:** Exposed `req.headers` and `req.cookies`.
- [x] **Custom Responses:** Introduced `rustapi.Response`.
- [x] **Structured `HTTPException`:** Mapped native Python exceptions to HTTP JSON responses.
- [x] **Automatic 422 Validation Errors:** Intercepted `pydantic.ValidationError` automatically.

---

## §2. Phase 2: Dependency Injection (Size: XL Epic) [IN PROGRESS]
*Goal: Implement a `Depends()` system.*
- [x] **Dependency Metadata Parsing:** `RouteDecorator` parses function signatures for `Depends` defaults.
- [x] **Dependency Map Storage:** Rust `RouteEntry` successfully caches dependency callables.
- [x] **Sync Resolver:** Route execution loop injects dependency results into `kwargs`.
- [x] **Async Resolver:** Execute `async def` dependencies on the Tokio reactor instead of blocking the thread.
- [x] **Context Managers (`yield`):** Support setup/teardown logic (e.g., closing DB sessions after the HTTP response is sent).
- [x] **Request Scoping:** Cache dependency results within a single request lifecycle (`use_cache=True`).

---

## §3. Phase 3: Rust-Native Database Engine (The ORM Bypass) (Size: XL) [NEXT]
*Goal: Bypass the Python GIL and standard ORM overhead entirely for database queries.*
- [ ] **Native Rust Connection Pool:** Embed `sqlx` inside the `Engine` to handle high-concurrency PostgreSQL/MySQL connections.
- [ ] **Zero-Copy JSON Streaming:** Execute SQL in Rust and stream the raw JSON bytes directly to the client socket (skipping Python dicts and Pydantic model allocations entirely).
- [ ] **Python Orchestration API:** Expose `app.connect_db()` and `app.db.query_json()` to the Python layer so users can write simple Python that executes at Rust speed.

---

## §4. Phase 4: Rust-Native Business Logic & Power Modules (Size: L)
*Goal: Provide pre-compiled Rust engines for heavy CPU-bound business logic so developers don't bottleneck the Python interpreter.*
- [ ] **PyO3 Escape Hatches:** Provide a formalized `rust_task` API so teams with Rust experience can easily write custom C-speed hot paths and attach them to RustAPI routes.
- [ ] **Rust Crypto & Auth Module:** Implement JWT validation and password hashing natively in Rust (using `jsonwebtoken` and `argon2` crates) exposed as high-level Python functions.
- [ ] **Rust Templating Engine:** Integrate `minijinja` (Rust-native Jinja2 clone) so HTML rendering happens entirely in C-memory.

---

## §5. Phase 5: Production Ergonomics (Size: L)
*Goal: Support the structural patterns used in large, multi-file codebases.*
- [ ] **Modular Routing (`APIRouter`):** Allow `engine.include_router(router, prefix="/api")` and merge them into the master Rust Radix tree.
- [ ] **Lifespan Hooks:** Support `@app.on_event("startup")` and `"shutdown"` to initialize DB pools before port binding.
- [ ] **Typed Parameter Coercion:** Automatically coerce string path/query variables to `int`, `float`, `bool`, or `UUID` based on Python function signatures, raising 422s on failure.

---

## §6. Architectural Boundaries & Trade-Offs
To maintain maximum performance, RustAPI strictly adheres to the following constraints:
1. **No ASGI Middleware:** RustAPI does not run under Uvicorn/Gunicorn. Standard ASGI middleware (like Starlette's) cannot be attached. Core middleware (Auth, CORS, Logging) must run in the Rust layer to preserve speed.
2. **The Python GIL Ceiling:** RustAPI eliminates framework overhead. It *does not* magically speed up poorly written Python code. If users write CPU-heavy Python loops, it will block a thread. They must use Phase 4 features (Rust Power Modules / Escape Hatches) to bypass the GIL for heavy computing.

___
#  PyRustAPI vs Robyn  [comparison]

### 1. The "Data Bottleneck" (The Ultimate Decider)                                                                                                                                    
                                                                                                                                                                                        
 This is where your Phase 3 (Rust-Native Database Engine) changes the game.                                                                                                             
                                                                                                                                                                                        
 - Robyn: While the engine is Rust, when a request hits a Python handler to fetch data, the data travels from the DB $\rightarrow$ Driver $\rightarrow$ Python Object $\rightarrow$     
   JSON $\rightarrow$ Socket. That "Python Object" step is the killer; it incurs heavy allocation and GIL overhead.                                                                     
 - PyRustAPI: Your goal of Zero-Copy JSON Streaming is a "killer feature." By executing SQL in Rust and streaming raw bytes directly to the socket, you bypass the Python interpreter for 
   the heaviest part of the request.                                                                                                                                                    
 - Verdict: PyRustAPI wins significantly on data-heavy workloads (large JSON payloads/big datasets).                                                                                      
                                                                                                                                                                                        
 ### 2. The "Compute Bottleneck" (CPU-Bound Tasks)                                                                                                                                      
                                                                                                                                                                                        
 - Robyn: If a user writes a complex image processing loop or a heavy math function in the Python route, they block the thread. They have to manually use run_in_executor or similar    
   patterns, which is "clunky."                                                                                                                                                         
 - PyRustAPI: Your Phase 4 (PyO3 Escape Hatches) provides a first-class way to write "hot paths" in Rust. By making it a core part of the framework's philosophy, you provide a formal    
   way to escape the GIL that is more integrated than Robyn's standard approach.                                                                                                        
 - Verdict: Tie / Slight edge to PyRustAPI for developer ergonomics and architectural integration.                                                                                        
                                                                                                                                                                                        
 ### 3. Ecosystem & Programmability                                                                                                                                                     
                                                                                                                                                                                        
 - Robyn: Being a more "standard" async Python framework, it plays well with the existing ecosystem, but it is still fundamentally limited by how much Python code the user writes.     
 - PyRustAPI: Your Phase 5 (Modular Routing & Lifespan Hooks) aims for "FastAPI-parity." This is crucial. If you achieve this, you solve the "Scale Problem." Developers don't just want  
   speed; they want to organize 50,000 lines of code into routers and modules.                                                                                                          
 - Verdict: Tie. If you hit Phase 5, you move from a "niche speed tool" to a "production-grade framework."                                                                              
                                                                                                                                                                                        
 ### 4. Architecture: The "Middleware" Philosophy                                                                                                                                       
                                                                                                                                                                                        
 - Robyn: Operates more like a traditional ASGI-lite engine.                                                                                                                            
 - PyRustAPI: Your decision to reject ASGI to preserve speed is bold and correct for your goal. By moving Auth and CORS into the Rust layer (Phase 4), you ensure that a "security check" 
   doesn't cost a context switch between the Rust engine and the Python interpreter.                                                                                                    
 - Verdict: PyRustAPI wins on pure efficiency by being "Opinionated about Speed.
 ___

# 📑 Strategic Position Report: PyRustAPI                                                                                                                                            
   **Project Codename:** PyRustAPI                                                                                                                                                      
   **Core Philosophy:** *Python for Developer Velocity, Rust for Execution Velocity.*                                                                                                   
                                                                                                                                                                                        
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 1. Executive Summary                                                                                                                                                              
   PyRustAPI is a high-performance, hybrid execution web framework. Unlike traditional frameworks that attempt to optimize Python code, PyRustAPI optimizes the **environment** in      
 which Python runs. By offloading high-latency operations (I/O, Serialization, Security, and Heavy Compute) to a native Rust engine, PyRustAPI minimizes the impact of the Python       
 Global Interpreter Lock (GIL) and provides a "Performance Slider" that allows developers to scale from simple Python logic to ultra-high-performance Rust hot-paths within a single    
 application.                                                                                                                                                                           
                                                                                                                                                                                        
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 2. The Competitive Landscape                                                                                                                                                      
                                                                                                                                                                                        
   | Metric | Standard Python (FastAPI/Flask) | Robyn (Fast Python) | **PyRustAPI (Hybrid Engine)** |                                                                                   
   | :--- | :--- | :--- | :--- |                                                                                                                                                        
   | **Primary Goal** | Ease of Use | Maximize Python speed | **Bypass Python bottlenecks** |                                                                                           
   | **Middleware** | Python-based (Slow) | Python-based (Fast) | **Rust-native (Instant)** |                                                                                           
   | **Data I/O** | High GIL Overhead | Moderate GIL Overhead | **Zero-Copy (No GIL)** |                                                                                                
   | **CPU-Bound** | Blocks the Event Loop | Manual offloading | **First-class Rust Integration** |                                                                                     
   | **Complexity** | Low | Low/Medium | **Medium (Tiered Complexity)** |                                                                                                               
                                                                                                                                                                                        
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 3. Technical Differentiation (The "Three Pillars")                                                                                                                                
                                                                                                                                                                                        
   ### I. The Data Bypass (The Killer Feature)                                                                                                                                          
   *   **The Problem:** In traditional frameworks, the "Data Tax" (SQL $\rightarrow$ Python Object $\rightarrow$ JSON $\rightarrow$ Socket) is the primary bottleneck in modern         
 microservices.                                                                                                                                                                         
   *   **The PyRustAPI Solution:** Through **Phase 3 (Zero-Copy JSON Streaming)**, data is processed in the Rust layer. The Python interpreter never "sees" the raw data; it only       
 orchestrates the request.                                                                                                                                                              
   *   **Impact:** Massive throughput gains for data-heavy APIs (Big Data, Analytics, IoT).                                                                                             
                                                                                                                                                                                        
   ### II. The Compute Escape Hatch (The Scalability Feature)                                                                                                                           
   *   **The Problem:** Complex business logic or heavy math in Python creates "The CPU Wall," where the server becomes unresponsive due to GIL contention.                             
   *   **The PyRustAPI Solution:** Through **Phase 4 (PyO3 Escape Hatches)**, developers can write critical logic in Rust and attach it to their Python routes as a "Hot Path."         
   *   **Impact:** Eliminates the need to rewrite an entire application in Rust just to solve a performance bottleneck.                                                                 
                                                                                                                                                                                        
   ### III. The Architectural Authority (The Reliability Feature)                                                                                                                       
   *   **The Problem:** Standard ASGI middleware adds cumulative latency to every request.                                                                                              
   *   **The PyRustAPI Solution:** By rejecting the ASGI standard in favor of a **Rust-Native Middleware Layer**, security (Auth/JWT) and plumbing (CORS/Logging) are handled before    
 the Python interpreter is even invoked.                                                                                                                                                
   *   **Impact:** A significantly lower "Latency Floor" and more predictable performance under load.                                                                                   
                                                                                                                                                                                        
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 4. Market Positioning & Target Domains                                                                                                                                            
                                                                                                                                                                                        
   PyRustAPI is not a general-purpose replacement for Flask; it is a specialized tool for **High-Performance Data Orchestration.**                                                      
                                                                                                                                                                                        
   ### **Target User Persona**                                                                                                                                                          
   *   **The Scaled Backend Engineer:** Managing high-load microservices who loves Python's syntax but is tired of fighting the GIL and JSON serialization overhead.                    
                                                                                                                                                                                        
   ### **Dominant Domains**                                                                                                                                                             
   1.  **High-Throughput Data APIs:** Services moving large JSON payloads or massive SQL result sets.                                                                                   
   2.  **High-Frequency Gateways:** API management layers requiring sub-millisecond security/auth checks.                                                                               
   3.  **Hybrid AI/ML Serving:** Deploying models where the "glue code" is Python, but the data preprocessing must be in Rust for speed.                                                
   4.  **Real-time Telemetry Ingestion:** High-concurrency ingestion of IoT or event-stream data.                                                                                       
                                                                                                                                                                                        
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 5. The "Performance Slider" (The Unique Selling Point)                                                                                                                            
                                                                                                                                                                                        
   PyRustAPI provides a tiered execution model that allows developers to choose their level of performance:                                                                             
                                                                                                                                                                                        
   1.  **Tier 1 (Orchestrator):** Write in **Pure Python** for maximum developer speed.                                                                                                 
   2.  **Tier 2 (Hybrid):** Use **Rust-Native Modules** for heavy I/O and Serialization (The "default" mode for high performance).                                                      
   3.  **Tier 3 (Turbo):** Write custom **Rust Hot-Paths** for extreme computational requirements.                                                                                                                                                                                                             
   ---                                                                                                                                                                                  
                                                                                                                                                                                        
   ## 6. Final Verdict                                                                                                                                                                  
   **Robyn** is a faster way to run Python.                                                                                                                                             
   **PyRustAPI** is a faster way to use Python.                                                                                                                                         
                                                                                                                                                                                        
   By shifting the architectural center of gravity from the Python Interpreter to the Rust Engine, PyRustAPI transforms the Python developer's biggest weakness (the GIL and object     
 overhead) into a managed resource. **PyRustAPI wins by making the "cost" of using Python negligible.** 

 ____

 # 🚀 RustAPI Engineering Roadmap

This document tracks the verified state of RustAPI and the phased implementation plan to reach feature-parity with production Python frameworks (like FastAPI) without sacrificing Rust's native speed.

## §0. Ground Truth (Already Shipped & Verified)
* [x] **Hyper 0.14 HTTP Engine & Radix Routing:** Core TCP socket binding, high-speed URL matching.
* [x] **Sync/Async Route Offloading:** `def` and `async def` routes executing securely inside Tokio.
* [x] **HTTP Essentials & OpenAPI:** Methods support, CORS, `/docs`, and `/openapi.json` generation.
* [x] **Phase 1: HTTP Metadata & Error Handling:** `req.headers`, `req.cookies`, Response objects, and automatic 422 validation via Pydantic.
* [x] **Phase 2: Dependency Injection & Generators:** FastAPI-style `Depends`, generator setup/teardown caching.
* [x] **Phase 5: Production Ergonomics:** Modular routing via `APIRouter` and lifespan hooks (`@app.on_event`).
* [x] **Advanced I/O & Real-Time:** Native multipart file uploads (`UploadFile`) and WebSockets.
* [x] **MCP Integration:** Model Context Protocol tools, resources, and prompts fully integrated.

---

## 🚀 NEXT UP: Phase 3 - Rust-Native Database Engine (The ORM Bypass)
*Goal: Bypass the Python GIL and standard ORM overhead entirely for high-concurrency database queries.*
`[████████████████████------------------] 50% — In Progress`
* [ ] **Native Rust Connection Pool:** Embed `sqlx` inside the `Engine` to handle high-concurrency PostgreSQL/MySQL connections safely across threads.
* [ ] **Zero-Copy JSON Streaming:** Execute SQL queries natively in Rust and stream raw UTF-8 JSON bytes directly to the client socket (skipping Python dicts and Pydantic model allocations entirely).
* [ ] **Python Orchestration API:** Expose `app.connect_db()` and `app.db.query_json()` to the Python layer.

---

## ⏳ UPCOMING: Phase 4 - Native Business Logic & Advanced Optimization
*Goal: Provide pre-compiled Rust engines for heavy CPU-bound business logic and push hardware limits.*
`[--------------------------------------] 0% — Pending`
* [ ] **PyO3 Escape Hatches:** Provide a formalized `rust_task` API so teams with Rust experience can easily write custom C-speed hot paths.
* [ ] **Rust Crypto & Auth Module:** Implement JWT validation and password hashing natively in Rust (`jsonwebtoken`, `argon2`).
* [ ] **Rust Templating Engine:** Integrate `minijinja` so HTML rendering happens entirely in C-memory.
* [ ] **Memory & Concurrency Tuning:** Integrate `mimalloc` and lock-free data structures (`DashMap`) for maximum multi-threaded throughput.
* [ ] **Telemetry & Metrics:** Add zero-allocation request tracing (`tracing`) and a native Prometheus metrics endpoint.

---

## § Architectural Boundaries & Trade-Offs
To maintain maximum performance, RustAPI strictly adheres to the following constraints:
* **No ASGI Middleware:** RustAPI does not run under Uvicorn/Gunicorn. Core middleware (Auth, CORS, Logging) must run in the Rust layer to preserve speed.
* **The Python GIL Ceiling:** RustAPI eliminates framework overhead, but cannot magically speed up poorly written Python code. CPU-heavy Python loops will block a thread. Phase 4 features must be used to bypass the GIL for heavy computing.
