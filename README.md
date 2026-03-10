# aiman

Local‑first LLM engine manager with a Rust host agent and a Vue dashboard. The host runs on LLM servers to start/stop engines and stream logs; the dashboard runs on a NAS to control multiple hosts over LAN.

## Features
- Start/stop engines (vLLM, llama.cpp, ktransformers, etc.) per config.
- Live log streaming and JSONL log/status history.
- Web UI config management (create/edit/delete configs per host).
- Web UI host management (add/update/remove hosts).
- Simple bearer‑token auth between dashboard and host.
- Nix‑first dev environment via `flake.nix`.

## Repository Layout
- `crates/host/` — Rust host binary (process supervisor + API).
- `crates/shared/` — Shared Rust types.
- `dashboard/` — Vue UI + Fastify server.
- `configs/` — Optional `hosts.toml` + `engines.toml` seed.
- `docs/` — Architecture notes.

## Quickstart (Host)
```bash
export AIMAN_API_KEY="dev-secret"
export AIMAN_BIND="0.0.0.0:4010"
export AIMAN_DATA_DIR="/path/to/aiman/data"
export AIMAN_CONFIG_STORE="/path/to/aiman/data/configs.json"
export AIMAN_ENGINES_CONFIG="/path/to/aiman/data/engines.toml"

cargo run -p aiman-host
```

The host keeps configs in `AIMAN_CONFIG_STORE` and the dashboard can add/update/remove them. If you want to seed configs from a TOML file on first launch, set `AIMAN_ENGINES_CONFIG` to an `engines.toml` path.

## Quickstart (Dashboard)
```bash
pnpm --dir dashboard install
pnpm --dir dashboard build
pnpm --dir dashboard run server
```

Open the UI at `http://<NAS_IP>:4020` and add hosts from the Hosts panel. If you want to seed hosts on first launch, set `AIMAN_HOSTS_CONFIG` to a `hosts.toml` path.

## Development
- Build Rust workspace: `cargo build`
- Run host: `cargo run -p aiman-host`
- Run UI dev server: `pnpm --dir dashboard dev`

## Notes
- Host API uses Axum 0.8 route params: `/v1/engines/{id}`.
- Logs, status snapshots, and the config store are stored in `AIMAN_DATA_DIR`.
- Dashboard hosts are stored in `AIMAN_HOSTS_STORE` (default `data/hosts.json`).

## Nix
Enter the dev shell:
```bash
nix develop
```
This provides Rust, Node.js, and pnpm.
