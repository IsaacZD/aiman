{
  description = "aiman - local LLM engine manager (agent + dashboard)";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      overlays = {
        default = final: prev: {
          aiman_agent = final.callPackage ./nix/aiman_agent.nix { };
          aiman_dashboard = final.callPackage ./nix/aiman_dashboard.nix { };
          aiman-dashboard-ui = final.callPackage ./nix/aiman-dashboard-ui.nix { };
          llama-benchy = final.callPackage ./nix/llama-benchy.nix { };
        };
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ overlays.default ]; };
      in
      {
        packages = {
          aiman_agent = pkgs.aiman_agent;
          aiman_dashboard = pkgs.aiman_dashboard;
          aiman-dashboard-ui = pkgs.aiman-dashboard-ui;
          llama-benchy = pkgs.llama-benchy;
          default = pkgs.aiman_agent;
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
            llama-benchy
          ];

          RUST_BACKTRACE = "1";

          # agent
          AIMAN_BIND = "127.0.0.1:4010";
          AIMAN_DATA_DIR = "./config/agent/data";
          # AIMAN_CONFIG_STORE = "./config/agent/data/configs.json";
          AIMAN_ENGINES_CONFIG = "./config/agent/engines.toml";
          # AIMAN_TOKIO_WORKERS = "2";
          # AIMAN_HARDWARE_TTL_SECS = "10";
          # AIMAN_HARDWARE_GPU_TIMEOUT_SECS = "2";
          # AIMAN_HARDWARE_SKIP_GPU = "0";
          # AIMAN_API_KEY = "";

          # dashboard
          AIMAN_HOSTS_CONFIG = "./config/dashboard/hosts.toml";
          AIMAN_DASHBOARD_UI_DIR = "./dashboard/dist/ui";
        };

        apps = {
          aiman_agent = flake-utils.lib.mkApp {
            drv = pkgs.writeShellApplication {
              name = "aiman_agent";
              runtimeInputs = with pkgs; [ cargo ];
              text = ''
                cargo run -p aiman_agent "$@"
              '';
            };
          };
          aiman_dashboard = flake-utils.lib.mkApp {
            drv = pkgs.writeShellApplication {
              name = "aiman_dashboard";
              runtimeInputs = with pkgs; [ cargo nodejs_20 ];
              text = ''
                npm --prefix dashboard install
                npm --prefix dashboard run build
                cargo run -p aiman_dashboard "$@"
              '';
            };
          };
        };
      }) // {
      overlays = overlays;
      nixosModules = {
        aiman_agent = import ./nix/modules/aiman_agent.nix;
        aiman-dashboard = import ./nix/modules/dashboard.nix;
      };
    };
}
