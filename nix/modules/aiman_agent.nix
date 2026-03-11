{ config, lib, pkgs, ... }:

let
  cfg = config.services.aiman_agent;
  envList = lib.mapAttrsToList (name: value: "${name}=${toString value}") cfg.environment;
  apiKeyEnv = lib.optional (cfg.apiKey != null) "AIMAN_API_KEY=${cfg.apiKey}";
  seedEnv = lib.optional (cfg.seedConfig != null) "AIMAN_ENGINES_CONFIG=${cfg.seedConfig}";
  configStore = cfg.configStore;
  bindAddr = cfg.bind;
  dataDir = cfg.dataDir;
  baseEnv = [
    "AIMAN_BIND=${bindAddr}"
    "AIMAN_DATA_DIR=${dataDir}"
    "AIMAN_CONFIG_STORE=${configStore}"
  ];
  fullEnv = baseEnv ++ apiKeyEnv ++ seedEnv ++ envList;

  portStr = lib.last (lib.splitString ":" bindAddr);
  hostPort = lib.tryEval (lib.toInt portStr);
  openFirewall = cfg.openFirewall && hostPort.success;
  isDefaultDataDir = cfg.dataDir == "/var/lib/aiman/agent";
  isDefaultStore = cfg.configStore == "/var/lib/aiman/agent/configs.json";
  tmpRules =
    (lib.optional isDefaultDataDir "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultStore "d /var/lib/aiman/agent 0750 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultStore "f ${cfg.configStore} 0640 ${cfg.user} ${cfg.group} -");

in
{
  options.services.aiman_agent = {
    enable = lib.mkEnableOption "aiman agent";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.aiman_agent;
      description = "aiman agent package.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "aiman";
      description = "User account to run the host service.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "aiman";
      description = "Group for the host service.";
    };

    dataDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/aiman/agent";
      description = "Data directory for log/status history and config store.";
    };

    configStore = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/aiman/agent/configs.json";
      description = "Path to the JSON config store used by the agent.";
    };

    seedConfig = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional engines.toml seed file path.";
    };

    bind = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0:4010";
      description = "Bind address for the agent API.";
    };

    apiKey = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Bearer token for agent authentication.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = {};
      description = "Additional environment variables for the agent service.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the agent API port in the firewall.";
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

    systemd.services.aiman_agent = {
      description = "aiman agent";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        ExecStart = "${cfg.package}/bin/aiman_agent";
        Environment = fullEnv;
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.tmpfiles.rules = tmpRules;

    networking.firewall.allowedTCPPorts = lib.mkIf openFirewall [ hostPort.value ];
  };
}
