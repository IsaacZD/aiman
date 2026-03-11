# aiman architecture

## Overview
- `aiman_agent` runs on each LLM server and owns engine lifecycles.
- `aiman-dashboard` runs on a NAS and connects to multiple hosts (<5).

## Data flow
- Dashboard calls agent REST endpoints for control and status.
- Agent exposes websocket endpoints to stream logs.

## Authentication
- Shared API key via `Authorization: Bearer <token>`.
- Optional TLS/mTLS can be added later for transport security.

## Config
- `configs-example/agent/engines.toml` defines engine presets (unique IDs).
- `configs-example/dashboard/hosts.toml` defines dashboard targets.
