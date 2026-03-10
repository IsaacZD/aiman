{
  description = "aiman - local LLM engine manager (host + dashboard)";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            just
            rustc
            cargo
            rustfmt
            clippy
            pkg-config
            openssl
            nodejs_20
            pnpm
          ];

          RUST_BACKTRACE = "1";

          # host
          AIMAN_BIND = "0.0.0.0:4010";
          AIMAN_DATA_DIR = "./mock/host/data";
          # AIMAN_CONFIG_STORE = "./mock/host/data/configs.json";
          AIMAN_ENGINES_CONFIG = "./mock/host/engines.toml";
          # AIMAN_API_KEY = "";

          # dashboard
          AIMAN_HOSTS_CONFIG = "../mock/dashboard/hosts.toml"; # need .. because the work dir is dashboard
        };

        apps = {
          aiman-host = flake-utils.lib.mkApp {
            drv = pkgs.writeShellApplication {
              name = "aiman-host";
              runtimeInputs = with pkgs; [ cargo ];
              text = ''
                cargo run -p aiman-host "$@"
              '';
            };
          };
          aiman-dashboard = flake-utils.lib.mkApp {
            drv = pkgs.writeShellApplication {
              name = "aiman-dashboard";
              runtimeInputs = with pkgs; [ nodejs_20 pnpm ];
              text = ''
                pnpm --dir dashboard install
                pnpm --dir dashboard build
                pnpm --dir dashboard server
              '';
            };
          };
        };
      });
}
