agent:
  cargo r

build-webui:
  npm --prefix dashboard run build

webui: build-webui
  npm --prefix dashboard run serve
