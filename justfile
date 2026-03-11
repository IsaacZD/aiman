agent:
  RUST_LOG=debug cargo r

build-webui:
  npm --prefix dashboard run build

webui: build-webui
  npm --prefix dashboard run serve

webui-dev:
  npm --prefix dashboard run dev
