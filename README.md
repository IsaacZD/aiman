# aiman

WARNING: This is a vibe coding project that mainly serves my personal needs. If it fits your needs feel free to give it a shot, but I may not be able to provide much support.

Local‑first LLM engine manager with Rust backends and a Vue dashboard. The agent runs on LLM servers to start/stop engines and stream logs; the dashboard runs on a NAS to control multiple hosts over LAN.

## Features
- Start/stop engines (vLLM, llama.cpp, ktransformers, etc.) per config.
- Live log streaming and JSONL log/status history.
- Web UI config management (create/edit/delete configs per host).
- Web UI host management (add/update/remove hosts).
- Simple bearer‑token auth between dashboard and agent.
- Nix‑first dev environment via `flake.nix`.

## Repository Layout
- `crates/aiman_agent/` — Rust agent binary (process supervisor + API).
- `crates/aiman_dashboard/` — Rust dashboard backend (proxies to agents, serves UI).
- `crates/shared/` — Shared Rust types and utilities (hardware info, storage, HTTP client).
- `dashboard/` — Vue UI (Vite build, served by dashboard backend).
- `configs-example/` — Sample seed configs (`agent/engines.toml`, `dashboard/hosts.toml`).
- `docs/` — Architecture notes.
- `nix/` — Nix packaging + NixOS modules.
- `data/` — Runtime JSONL logs/status history + stores (generated at runtime; ignored by git).

## Quickstart (Agent)
```bash
export AIMAN_API_KEY="dev-secret"
export AIMAN_BIND="0.0.0.0:4010"
export AIMAN_DATA_DIR="/path/to/data"
export AIMAN_CONFIG_STORE="/path/to/configs.json"
export AIMAN_ENGINES_CONFIG="/path/to/engines.toml"
export AIMAN_TOKIO_WORKERS="2"
export AIMAN_HARDWARE_TTL_SECS="10"
export AIMAN_HARDWARE_GPU_TIMEOUT_SECS="2"
export AIMAN_HARDWARE_SKIP_GPU="0"

cargo run -p aiman_agent
```

The agent keeps configs in `AIMAN_CONFIG_STORE` and the dashboard can add/update/remove them. If you want to seed configs from a TOML file on first launch, set `AIMAN_ENGINES_CONFIG` to an `engines.toml` path.

## Quickstart (Dashboard)
```bash
# Build the Vue UI
bun --cwd dashboard install
bun --cwd dashboard run build

# Run the Rust dashboard backend
export AIMAN_DASHBOARD_PORT="4020"
export AIMAN_DASHBOARD_BIND="0.0.0.0"
cargo run -p aiman_dashboard
```

Open the UI at `http://<NAS_IP>:4020` and add hosts from the Hosts panel. If you want to seed hosts on first launch, set `AIMAN_HOSTS_CONFIG` to a `hosts.toml` path (for example `configs-example/dashboard/hosts.toml`).

## Development
- Build Rust workspace: `cargo build`
- Run agent: `cargo run -p aiman_agent`
- Run dashboard backend: `cargo run -p aiman_dashboard`
- Run UI dev server (hot reload): `bun --cwd dashboard run dev`

For development, you can copy seed configs into `config/` (the dev shell paths in `flake.nix`):
```bash
cp -n configs-example/agent/engines.toml config/agent/engines.toml
cp -n configs-example/dashboard/hosts.toml config/dashboard/hosts.toml
```
The dev shell in `flake.nix` points `AIMAN_ENGINES_CONFIG` at `./config/agent/engines.toml` and `AIMAN_HOSTS_CONFIG` at `./config/dashboard/hosts.toml`. Update those env vars if you want different paths.

## Notes
- Agent API uses Axum 0.8 route params: `/v1/engines/{id}`.
- Logs, status snapshots, and the config store live under `AIMAN_DATA_DIR` (default `data`).
- Dashboard hosts are stored in `AIMAN_HOSTS_STORE` (default `data/hosts.json`).
- Dashboard benchmark history is stored in `AIMAN_DASHBOARD_BENCHMARKS` (default `data/benchmarks-dashboard.jsonl`).
- Agent hardware refresh tuning: `AIMAN_HARDWARE_TTL_SECS`, `AIMAN_HARDWARE_GPU_TIMEOUT_SECS`, `AIMAN_HARDWARE_SKIP_GPU`.
- Agent CPU cap (agent runtime only): `AIMAN_TOKIO_WORKERS`.

## Nix
### Dev shell workflow
The dev shell provides Rust + Bun and preconfigures local env vars for agent + dashboard development.
```bash
nix develop
```
If you haven't seeded configs yet:
```bash
cp -n configs-example/agent/engines.toml config/agent/engines.toml
cp -n configs-example/dashboard/hosts.toml config/dashboard/hosts.toml
```

This provides Rust and Bun.

### Packages
- `packages.aiman_agent` builds the agent binary.
- `packages.aiman_dashboard` builds the dashboard backend binary.
- `packages.aiman-dashboard-ui` builds the Vue UI (uses Bun via `stdenv.mkDerivation`).

### NixOS Modules
Enable the services and overlay in your system config:
```nix
{
  nixpkgs.overlays = [ inputs.aiman.overlays.default ];

  services.aiman_agent = {
    enable = true;
    apiKey = "dev-secret";
    openFirewall = true;
  };

  services.aiman-dashboard = {
    enable = true;
    openFirewall = true;
  };
}
```

Key options:
- Agent: `services.aiman_agent.dataDir`, `configStore`, `seedConfig`, `bind`, `apiKey`, `tokioWorkers`, `hardwareTtlSecs`, `hardwareGpuTimeoutSecs`, `hardwareSkipGpu`.
- Dashboard: `services.aiman-dashboard.hostsStore`, `hostsConfig`, `bind`, `port`.
