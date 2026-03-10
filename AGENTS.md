# Repository Guidelines

## Project Structure & Module Organization
- `crates/host/` — Rust binary that runs on LLM servers (process supervision, API, log streaming).
- `crates/shared/` — Shared Rust types used by host and dashboard.
- `dashboard/src/server/` — Fastify server that proxies host APIs and serves the UI.
- `dashboard/src/ui/` — Vue 3 UI (Vite).
- `configs/` — Host and engine TOML configs (`hosts.toml`, `engines.toml`).
- `docs/` — Architecture notes and design context.
- `data/` — Runtime JSONL logs/status history (generated at runtime; ignored by git).

## Build, Test, and Development Commands
- `cargo build` — Build the Rust workspace.
- `cargo run -p aiman-host` — Run the host API locally.
- `pnpm --dir dashboard install` — Install dashboard dependencies.
- `pnpm --dir dashboard dev` — Run the Vite UI dev server.
- `pnpm --dir dashboard build` — Build the production UI assets.
- `pnpm --dir dashboard server` — Run the dashboard Fastify server.

## Coding Style & Naming Conventions
- Rust: follow `rustfmt` defaults; use `snake_case` for functions and `CamelCase` for types.
- TypeScript/Vue: keep components in `PascalCase` filenames and use `camelCase` for functions.
- Keep modules small and purpose‑driven; prefer clear names over abbreviations.

## Testing Guidelines
- No automated tests are set up yet. If you add tests, keep them close to the module they cover.
- Suggested future conventions: `*_test.rs` for Rust and `*.spec.ts` for UI/server tests.

## Commit & Pull Request Guidelines
- History is minimal; no strict commit convention exists yet.
- Use concise, imperative commit subjects (e.g., “Add engine log history endpoint”).
- PRs should include a short summary, testing notes, and screenshots for UI changes.

## Security & Configuration Notes
- Host auth uses a bearer token if `AIMAN_API_KEY` is set.
- Config paths:
  - Host: `AIMAN_ENGINES_CONFIG`, `AIMAN_DATA_DIR`
  - Dashboard: `AIMAN_HOSTS_CONFIG`, `AIMAN_DASHBOARD_PORT`, `AIMAN_DASHBOARD_BIND`
- Treat configs as LAN‑only but avoid committing secrets.
