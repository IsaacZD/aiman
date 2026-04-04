# Plan: Docker-based engine support

## Goals
- Add a Docker-backed engine type in Aimán.
- Allow optional custom image builds via a user-provided Dockerfile when no existing image fits.
- Keep shared schema changes in sync across `crates/shared`, `dashboard/src/server`, and `dashboard/src/ui`.

## Scope assumptions
- Docker is available on the host where `aiman_agent` runs.
- Engine definitions live in `config/agent/engines.toml` (or its configured path).
- Dashboard and agent APIs already support multiple engine types.

## Step 1: Discover current engine model & extension points
- Inspect current engine config TOML schema and parsing in `crates/shared` and `crates/aiman_agent`.
- Identify how engine types are discriminated (enum/tag field) and how lifecycle is handled.
- Locate any dashboard UI forms or JSON schema used for engine config editing.

Deliverable:
- Notes on existing engine config fields and where to add a Docker variant.

## Step 2: Design Docker engine config schema
Add a new engine type (e.g., `type = "docker"`) with fields such as:
- `image`: string (required if `build` is not provided)
- `build` (optional table):
  - `context`: path to build context (default: engine config directory)
  - `dockerfile`: relative path to Dockerfile (optional; allow custom)
  - `target`: optional build target
  - `build_args`: map<string, string>
  - `platform`: optional platform
- `command`: optional array/strings for container entrypoint override
- `env`: map<string, string>
- `ports`: list of `{host, container}` or string `host:container`
- `volumes`: list of `{host, container, ro}` or string `host:container[:ro]`
- `labels`: map<string, string>
- `network`: optional network name
- `healthcheck`: optional health probe (reuse existing if present)

Decide:
- Whether `image` can be omitted when `build` is set.
- Where to resolve relative paths (config file dir vs cwd).

Deliverables:
- Updated shared Rust types + serde tags in `crates/shared`.
- Updated parsing/validation with good error messages.

## Step 3: Agent implementation (Docker lifecycle)
- Add a Docker engine runtime implementation in `crates/aiman_agent`:
  - Build or pull image:
    - If `build` present → `docker build` (support custom Dockerfile).
    - Else ensure image exists (pull if missing).
  - Run container with configured env/ports/volumes/labels.
  - Track container ID and map to engine instance.
  - Stop/remove container on shutdown.
  - Stream logs via `docker logs -f` or API client if available.
  - Health checks: map to container health status or existing probe mechanism.
- Decide between shelling out to docker CLI vs using a Docker API crate; prefer CLI initially for simplicity unless project already uses a crate.

Deliverables:
- New Docker engine module with start/stop/log/health.
- Errors mapped to existing status/log conventions.

## Step 4: Dashboard server + UI
- Server: allow Docker engine configs in API schemas and validators.
- UI: add Docker engine form fields (image/build/Dockerfile/ports/volumes/env).
- Keep defaults minimal and safe; include Dockerfile override.

Deliverables:
- UI form changes in `dashboard/src/ui`.
- API schema validation updates in `dashboard/src/server`.

## Step 5: Docs + examples
- Add example Docker engine config to `configs-example/agent/engines.toml`.
- Document minimal Docker engine usage and custom Dockerfile build.
- Mention any prerequisites (Docker daemon, permissions).

Deliverables:
- Updated example config and short doc notes in `docs/`.

## Acceptance criteria
- A Docker engine can run from a public image with env/ports/volumes configured.
- A Docker engine can build from a custom Dockerfile and run successfully.
- Agent reports healthy status and streams logs.
- Dashboard can view/create Docker engine configs without breaking other engine types.

## Risks / open questions
- Docker availability/permissions on host.
- Path resolution for build context and host volumes.
- Cross-platform differences (Linux vs macOS Docker path mapping).
- Engine lifecycle + cleanup on agent restart.
