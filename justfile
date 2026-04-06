# Run the agent
agent:
  RUST_LOG=debug cargo run -p aiman_agent

# Build the Vue UI
build-ui:
  bun --cwd dashboard run build

# Run the dashboard backend (builds UI first)
dashboard: build-ui
  RUST_LOG=debug cargo run -p aiman_dashboard

# Run just the dashboard backend (assumes UI is already built)
dashboard-only:
  RUST_LOG=debug cargo run -p aiman_dashboard

# Run the Vite dev server for UI hot reload
ui-dev:
  bun --cwd dashboard run dev

# Build both Rust binaries
build:
  cargo build -p aiman_agent -p aiman_dashboard

# Run clippy on the workspace
lint:
  cargo clippy --workspace -- -D warnings

# Run both agent and dashboard (in foreground, agent first)
all: build-ui
  @echo "Run 'just agent' and 'just dashboard-only' in separate terminals"

# Clean build artifacts
clean:
  cargo clean
  rm -rf dashboard/dist

# Clean everything including node_modules and bun.lockb
clean-all: clean
  rm -rf dashboard/node_modules dashboard/bun.lockb
