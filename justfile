agent:
  cargo r

build-webui:
  pnpm --dir dashboard build

webui: build-webui
  pnpm --dir dashboard serve
