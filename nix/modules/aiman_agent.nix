{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.aiman_agent;
  envList = lib.mapAttrsToList (name: value: "${name}=${toString value}") cfg.environment;
  apiKeyEnv = lib.optional (cfg.apiKey != null) "AIMAN_API_KEY=${cfg.apiKey}";
  seedEnv = lib.optional (cfg.seedConfig != null) "AIMAN_ENGINES_CONFIG=${cfg.seedConfig}";
  tokioEnv = lib.optional (cfg.tokioWorkers != null) "AIMAN_TOKIO_WORKERS=${toString cfg.tokioWorkers}";
  hardwareTtlEnv = lib.optional (cfg.hardwareTtlSecs != null) "AIMAN_HARDWARE_TTL_SECS=${toString cfg.hardwareTtlSecs}";
  hardwareGpuTimeoutEnv =
    lib.optional (cfg.hardwareGpuTimeoutSecs != null)
    "AIMAN_HARDWARE_GPU_TIMEOUT_SECS=${toString cfg.hardwareGpuTimeoutSecs}";
  hardwareSkipGpuEnv =
    lib.optional (cfg.hardwareSkipGpu != null)
    "AIMAN_HARDWARE_SKIP_GPU=${
      if cfg.hardwareSkipGpu
      then "1"
      else "0"
    }";
  nvmlEnv =
    lib.optional (cfg.nvmlLibraryPath != null)
    "LD_LIBRARY_PATH=${cfg.nvmlLibraryPath}";
  configStore = cfg.configStore;
  dataDir = cfg.dataDir;
  baseEnv = [
    "AIMAN_BIND=${cfg.host}:${toString cfg.port}"
    "AIMAN_DATA_DIR=${dataDir}"
    "AIMAN_CONFIG_STORE=${configStore}"
  ];
  fullEnv = baseEnv ++ apiKeyEnv ++ seedEnv ++ tokioEnv ++ hardwareTtlEnv ++ hardwareGpuTimeoutEnv ++ hardwareSkipGpuEnv ++ nvmlEnv ++ envList;

  isDefaultDataDir = cfg.dataDir == "/var/lib/aiman/agent";
  isDefaultStore = cfg.configStore == "/var/lib/aiman/agent/configs.json";
  tmpRules =
    (lib.optional isDefaultDataDir "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultStore "d /var/lib/aiman/agent 0750 ${cfg.user} ${cfg.group} -")
    ++ (lib.optional isDefaultStore "f ${cfg.configStore} 0640 ${cfg.user} ${cfg.group} -");
in {
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
      description = "User account to run the agent service.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "aiman";
      description = "Group for the agent service.";
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

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Host ipv4 address for the agent API.";
    };

    port = lib.mkOption {
      type = lib.types.int;
      default = 4010;
      description = "Port for the agent API.";
    };

    apiKey = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Bearer token for agent authentication.";
    };

    tokioWorkers = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = "Limit the agent runtime worker threads (agent only; does not affect engines).";
    };

    hardwareTtlSecs = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = "Cache duration in seconds for hardware info refresh.";
    };

    hardwareGpuTimeoutSecs = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = "Timeout in seconds for GPU probe commands.";
    };

    hardwareSkipGpu = lib.mkOption {
      type = lib.types.nullOr lib.types.bool;
      default = null;
      description = "Skip GPU probing in hardware info.";
    };

    nvmlLibraryPath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Path to the directory containing libnvidia-ml.so, enabling richer
        GPU metrics via NVML (utilization, temperature, power). When null,
        the agent falls back to nvidia-smi or lspci.
        Example: "''${config.hardware.nvidia.package}/lib"
      '';
    };

    extraPackages = lib.mkOption {
      type = lib.types.listOf lib.types.package;
      default = [];
      description = "Extra packages added to PATH so the agent can spawn engine binaries (e.g. pkgs.llama-cpp).";
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
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      path = cfg.extraPackages ++ [pkgs.podman];
      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        ExecStart = "${cfg.package}/bin/aiman_agent";
        Environment = fullEnv ++ [
          # Prepend wrappers for newuidmap/newgidmap (required by rootless podman)
          "PATH=/run/wrappers/bin:${lib.makeBinPath (cfg.extraPackages ++ [pkgs.podman])}"
        ];
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.tmpfiles.rules = tmpRules;

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [cfg.port];
  };
}
