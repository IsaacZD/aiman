{
  description = "aiman - local LLM engine manager (host + dashboard)";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      overlays = {
        default = final: prev: {
          aiman-host = final.callPackage ./nix/aiman-host.nix { };
          aiman-dashboard = final.callPackage ./nix/aiman-dashboard.nix { };
        };
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ overlays.default ]; };
      in
      {
        packages = {
          aiman-host = pkgs.aiman-host;
          aiman-dashboard = pkgs.aiman-dashboard;
          default = pkgs.aiman-host;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            node2nix
            just
            rustc
            cargo
            rustfmt
            clippy
            pkg-config
            openssl
            nodejs_20
          ];

          RUST_BACKTRACE = "1";

          # host
          AIMAN_BIND = "127.0.0.1:4010";
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
              runtimeInputs = with pkgs; [ nodejs_20 ];
              text = ''
                npm --prefix dashboard install
                npm --prefix dashboard run build
                npm --prefix dashboard run serve
              '';
            };
          };
        };
      }) // {
      overlays = overlays;
      nixosModules = {
        aiman-host = import ./nix/modules/host.nix;
        aiman-dashboard = import ./nix/modules/dashboard.nix;
      };
    };
}
