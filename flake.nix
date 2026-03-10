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
