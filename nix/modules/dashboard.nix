{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.aiman-dashboard;
  envList = lib.mapAttrsToList (name: value: "${name}=${toString value}") cfg.environment;
  baseEnv = [
    "AIMAN_DASHBOARD_BIND=${cfg.bind}"
    "AIMAN_DASHBOARD_PORT=${toString cfg.port}"
    "AIMAN_HOSTS_STORE=${cfg.hostsStore}"
    "AIMAN_DASHBOARD_UI_DIR=${cfg.uiPackage}/share/aiman-dashboard-ui/ui"
  ];
  seedEnv = lib.optional (cfg.hostsConfig != null) "AIMAN_HOSTS_CONFIG=${cfg.hostsConfig}";
  benchmarksEnv = lib.optional (cfg.benchmarksPath != null) "AIMAN_DASHBOARD_BENCHMARKS=${cfg.benchmarksPath}";
  fullEnv = baseEnv ++ seedEnv ++ benchmarksEnv ++ envList;
  openFirewall = cfg.openFirewall;
  isDefaultStore = cfg.hostsStore == "/var/lib/aiman/dashboard/hosts.json";
  isDefaultBenchmarks = cfg.benchmarksPath == "/var/lib/aiman/dashboard/benchmarks.jsonl";
  tmpRules =
    (lib.optional isDefaultStore "d /var/lib/aiman/dashboard 0750 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultStore "f ${cfg.hostsStore} 0640 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultBenchmarks "f ${cfg.benchmarksPath} 0640 ${cfg.user} ${cfg.group} -");
in {
  options.services.aiman-dashboard = {
    enable = lib.mkEnableOption "aiman dashboard server";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.aiman_dashboard;
      description = "aiman dashboard backend package.";
    };

    uiPackage = lib.mkOption {
      type = lib.types.package;
      default = pkgs.aiman-dashboard-ui;
      description = "aiman dashboard UI package.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "aiman";
      description = "User account to run the dashboard service.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "aiman";
      description = "Group for the dashboard service.";
    };

    bind = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Bind address for the dashboard HTTP server.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 4020;
      description = "Port for the dashboard HTTP server.";
    };

    hostsStore = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/aiman/dashboard/hosts.json";
      description = "JSON store for managed hosts.";
    };

    hostsConfig = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional hosts.toml seed file path.";
    };

    benchmarksPath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = "/var/lib/aiman/dashboard/benchmarks.jsonl";
      description = "Path to store benchmark results.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = {};
      description = "Additional environment variables for the dashboard service.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the dashboard HTTP port in the firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users = lib.mkIf (cfg.user == "aiman") {
      aiman = {
        isSystemUser = true;
        group = cfg.group;
      };
    };

    users.groups = lib.mkIf (cfg.group == "aiman") {
      aiman = {};
    };

    systemd.services.aiman-dashboard = {
      description = "aiman dashboard server";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      path = with pkgs; [bash llama-benchy];
      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        ExecStart = "${cfg.package}/bin/aiman_dashboard";
        Environment = fullEnv;
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.tmpfiles.rules = tmpRules;

    networking.firewall.allowedTCPPorts = lib.mkIf openFirewall [cfg.port];
  };
}
