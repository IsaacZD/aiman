# Progress Tracker: Docker-based engine support

Last updated: 2026-03-15 (UTC)

> Status legend: NOT STARTED | IN PROGRESS | DONE | BLOCKED

## 1) Discover current engine model & extension points
Status: DONE

- [x] Locate engine config schema in `crates/shared` (Rust types + serde tags).
- [x] Find engine config parsing/validation in `crates/aiman_agent`.
- [x] Identify runtime interface for engines (start/stop/log/health).
- [x] Find dashboard server schema/validators for engine configs in `dashboard/src/server`.
- [x] Find dashboard UI engine config form components in `dashboard/src/ui`.
- [x] Note existing fields that can be reused (env, ports, health checks, log streaming).

Artifacts
- Notes file (if needed): `.agents/codex/tasks/planning/0001_docker_support/discovery_notes.md`

## 2) Design Docker engine config schema
Status: DONE

- [x] Draft Docker engine TOML example (image-only).
- [x] Draft Docker engine TOML example (build with custom Dockerfile).
- [x] Decide required vs optional fields (`image` vs `build`).
- [x] Decide path resolution rules for `build.context` and `build.dockerfile`.
- [x] Define env, ports, volumes, labels, network structure.
- [x] Align healthcheck with existing model (or document new fields).
- [x] Update Rust shared types in `crates/shared`.
- [x] Update server-side validation in `dashboard/src/server`.

Artifacts
- Example config in `configs-example/agent/engines.toml` (later in step 5)
- Schema notes: `.agents/codex/tasks/planning/0001_docker_support/schema_notes.md`

## 3) Implement Docker engine lifecycle in agent
Status: DONE

- [x] Choose Docker integration strategy (CLI vs API crate).
- [x] Implement image build (with custom Dockerfile, build args, target).
- [x] Implement image pull (if no build).
- [x] Implement container run (env/ports/volumes/labels/network).
- [x] Track container IDs per engine instance. (container name defaults to config id)
- [x] Implement stop/remove and cleanup on shutdown.
- [x] Implement log streaming integration.
- [ ] Implement health status mapping.
- [x] Add error handling + status propagation.

Artifacts
- New module(s) in `crates/aiman_agent` for Docker engine runtime.

## 4) Wire into dashboard server & UI
Status: DONE

- [x] Extend API schema to accept Docker engine configs.
- [x] Ensure config serialization/deserialization works end-to-end.
- [x] Add UI form fields for Docker engine options.
- [ ] Provide UI help text for Dockerfile override + build context.
- [x] Confirm existing engine types unaffected.

Artifacts
- UI changes in `dashboard/src/ui`.
- API changes in `dashboard/src/server`.

## 5) Docs + examples
Status: IN PROGRESS

- [x] Add Docker engine example to `configs-example/agent/engines.toml`.
- [ ] Add short docs in `docs/` for Docker usage + custom Dockerfile.
- [ ] Mention Docker daemon requirement and permissions.

Artifacts
- Updated example config + doc notes.

## Acceptance criteria checklist
- [ ] Run engine from public image with env/ports/volumes.
- [ ] Build and run engine from custom Dockerfile.
- [ ] Agent shows healthy status and streams logs.
- [ ] Dashboard can view/create Docker engine configs.

## Open questions / risks
- [ ] Docker daemon availability and permissions on host.
- [ ] Path resolution for build context and volume mounts.
- [ ] Cross-platform path mapping (Linux vs macOS).
- [ ] Cleanup behavior on agent restart.
