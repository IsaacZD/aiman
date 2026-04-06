# Repository Guidelines

## Project Structure & Module Organization
- `crates/aiman_agent/` — Rust binary that runs on LLM servers (process supervision, API, log streaming).
- `crates/aiman_dashboard/` — Rust binary that proxies agent APIs and serves the Vue UI.
- `crates/shared/` — Shared Rust types and utilities (hardware info, storage, HTTP client, error handling).
- `dashboard/src/ui/` — Vue 3 UI (Vite).
- `configs-example/` — Sample host/engine TOML configs (`dashboard/hosts.toml`, `agent/engines.toml`).
- `docs/` — Architecture notes and design context.
- `nix/` — Nix packaging + NixOS modules.
- `data/` — Runtime JSONL logs/status history + stores (generated at runtime; ignored by git).
- `dashboard/dist/` — Built UI assets (avoid manual edits; regenerate via build).

## Build, Test, and Development Commands
- `cargo build` — Build the Rust workspace.
- `cargo run -p aiman_agent` — Run the agent API locally.
- `cargo run -p aiman_dashboard` — Run the dashboard backend locally.
- `npm --prefix dashboard install` — Install dashboard UI dependencies.
- `npm --prefix dashboard run dev` — Run the Vite UI dev server (hot reload).
- `npm --prefix dashboard run build` — Build the production UI assets.
- For local dev, copy the seeds into `config/` if you want to use the dev shell defaults, or update the dev shell env vars in `flake.nix`.

## Coding Style & Naming Conventions
- Rust: follow `rustfmt` defaults; use `snake_case` for functions and `CamelCase` for types.
- Vue: keep components in `PascalCase` filenames and use `camelCase` for functions.
- Keep modules small and purpose‑driven; prefer clear names over abbreviations.
- Keep shared schema changes in sync across `crates/shared` and `dashboard/src/ui`.

## Documentation & Comments
- Add detailed, intentional comments for non-obvious logic and design decisions.
- Prefer brief module-level notes for cross-cutting behavior, plus inline comments at tricky spots.
- Avoid redundant comments for self-evident code; focus on why, not just what.

## Testing Guidelines
- No automated tests are set up yet. If you add tests, keep them close to the module they cover.
- Suggested future conventions: `*_test.rs` for Rust and `*.spec.ts` for UI tests.

## Commit & Pull Request Guidelines
- History is minimal; no strict commit convention exists yet.
- Use concise, imperative commit subjects (e.g., “Add engine log history endpoint”).
- PRs should include a short summary, testing notes, and screenshots for UI changes.

## Security & Configuration Notes
- Agent auth uses a bearer token if `AIMAN_API_KEY` is set.
- Config paths:
  - Agent: `AIMAN_API_KEY`, `AIMAN_BIND`, `AIMAN_DATA_DIR`, `AIMAN_CONFIG_STORE`, `AIMAN_ENGINES_CONFIG`
  - Dashboard: `AIMAN_HOSTS_CONFIG`, `AIMAN_HOSTS_STORE`, `AIMAN_DASHBOARD_BENCHMARKS`, `AIMAN_DASHBOARD_PORT`, `AIMAN_DASHBOARD_BIND`
- Treat configs as LAN‑only but avoid committing secrets.
