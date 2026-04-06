# aiman architecture

## Overview
- `aiman_agent` runs on each LLM server and owns engine lifecycles.
- `aiman_dashboard` runs on a NAS and connects to multiple hosts (<5).
- Both are Rust binaries sharing common code via `aiman-shared`.

## Crate Structure

```
crates/
├── shared/src/           # Shared types and utilities
│   ├── lib.rs            # Re-exports, data contracts (EngineConfig, etc.)
│   ├── hardware.rs       # HardwareInfo, GpuInfo types
│   ├── storage.rs        # LogWriter, JSONL read/write utilities
│   ├── http.rs           # ProxyClient for HTTP proxying (feature: http)
│   └── error.rs          # CommonError, ApiError trait
│
├── aiman_agent/src/      # Agent binary
│   ├── main.rs           # Axum server setup
│   ├── state.rs          # AppState with engine supervisor
│   ├── api.rs            # REST/WebSocket/SSE handlers
│   ├── hardware.rs       # Hardware info collection (uses shared types)
│   ├── supervisor/       # Engine process orchestration
│   └── benchmark.rs      # LLM benchmarking
│
└── aiman_dashboard/src/  # Dashboard binary
    ├── main.rs           # Axum server + static file serving
    ├── state.rs          # AppState with hosts and proxy client
    ├── hosts.rs          # Host config loading/persistence
    ├── types.rs          # Dashboard-specific types
    └── api/
        ├── hosts.rs      # Host CRUD handlers
        ├── proxy.rs      # Proxy requests to agents
        ├── streaming.rs  # SSE/WebSocket bridges
        ├── benchmark.rs  # Run benchmarks via llama-benchy
        └── aggregation.rs # Cross-host data aggregation
```

## Data flow
- Dashboard calls agent REST endpoints for control and status.
- Agent exposes SSE endpoint (`/v1/events`) for real-time status updates.
- Agent exposes WebSocket endpoint (`/v1/engines/{id}/logs/ws`) to stream logs.
- Dashboard bridges these streams to the UI.

## Authentication
- Shared API key via `Authorization: Bearer <token>`.
- Optional TLS/mTLS can be added later for transport security.

## Config
- `configs-example/agent/engines.toml` defines engine presets (unique IDs).
- `configs-example/dashboard/hosts.toml` defines dashboard targets.

## Shared Module Details

### hardware.rs
Types for hardware information collected by the agent and displayed by the dashboard:
- `HardwareInfo`: hostname, CPU, memory, GPU list
- `GpuInfo`: name, memory, utilization, temperature

### storage.rs
JSONL utilities used by both agent (logs, status) and dashboard (benchmarks):
- `LogWriter`: batched async JSONL writer with channel-based buffering
- `read_jsonl<T>()`: read and deserialize JSONL with optional filtering
- `atomic_write_sync()`: atomic file writes via temp file + rename

### http.rs (feature: http)
HTTP client utilities for the dashboard to proxy requests to agents:
- `ProxyClient`: wraps reqwest with timeout and auth handling
- `request<T>()`: JSON request/response with error mapping
- `stream_sse()`: SSE stream for event bridging

### error.rs
Common error handling patterns:
- `ApiError` trait: maps errors to HTTP status codes
- `CommonError`: NotFound, BadRequest, Conflict, Internal, etc.

## Strict Compiler Checks
The workspace uses strict Clippy lints to catch errors at compile time:
- `unwrap_used = "deny"`: forces proper error handling
- `panic = "deny"`: no panics in library code
- `unsafe_code = "deny"`: explicit unsafe blocks only where necessary
