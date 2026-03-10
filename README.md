# aiman

Local‑first LLM engine manager with a Rust host agent and a Vue dashboard. The host runs on LLM servers to start/stop engines and stream logs; the dashboard runs on a NAS to control multiple hosts over LAN.

## Features
- Start/stop engines (vLLM, llama.cpp, ktransformers, etc.) per config.
- Live log streaming and JSONL log/status history.
- Simple bearer‑token auth between dashboard and host.
- Nix‑first dev environment via `flake.nix`.

## Repository Layout
- `crates/host/` — Rust host binary (process supervisor + API).
- `crates/shared/` — Shared Rust types.
- `dashboard/` — Vue UI + Fastify server.
- `configs/` — `engines.toml` and `hosts.toml`.
- `docs/` — Architecture notes.

## Quickstart (Host)
```bash
export AIMAN_API_KEY="dev-secret"
export AIMAN_BIND="0.0.0.0:4010"
export AIMAN_ENGINES_CONFIG="/path/to/aiman/configs/engines.toml"
export AIMAN_DATA_DIR="/path/to/aiman/data"

cargo run -p aiman-host
```

## Quickstart (Dashboard)
```bash
pnpm --dir dashboard install
pnpm --dir dashboard build
pnpm --dir dashboard run server
```

Update `configs/hosts.toml` on the dashboard machine:
```toml
[[host]]
id = "llm-01"
name = "LLM Server 01"
base_url = "http://<HOST_LAN_IP>:4010"
api_key = "dev-secret"
```

Open the UI at `http://<NAS_IP>:4020`.

## Development
- Build Rust workspace: `cargo build`
- Run host: `cargo run -p aiman-host`
- Run UI dev server: `pnpm --dir dashboard dev`

## Notes
- Host API uses Axum 0.8 route params: `/v1/engines/{id}`.
- Logs and status snapshots are stored as JSONL in `AIMAN_DATA_DIR`.

## Nix
Enter the dev shell:
```bash
nix develop
```
This provides Rust, Node.js, and pnpm.
