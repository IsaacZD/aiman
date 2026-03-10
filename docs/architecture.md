# aiman architecture

## Overview
- `aiman-host` runs on each LLM server and owns engine lifecycles.
- `aiman-dashboard` runs on a NAS and connects to multiple hosts (<5).

## Data flow
- Dashboard calls host REST endpoints for control and status.
- Host exposes websocket endpoints to stream logs.

## Authentication
- Shared API key via `Authorization: Bearer <token>`.
- Optional TLS/mTLS can be added later for transport security.

## Config
- `configs/engines.toml` defines engine presets (unique IDs).
- `configs/hosts.toml` defines dashboard targets.
