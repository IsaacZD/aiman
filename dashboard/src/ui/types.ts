export type Host = {
  id: string;
  name: string;
  base_url: string;
  api_key?: string;
  model_libraries?: string[];
};

export type EnvVar = {
  key: string;
  value: string;
};

export type EngineConfig = {
  id: string;
  name: string;
  // Keep in sync with server + shared EngineType for round-trip safety.
  engine_type:
    | "Vllm"
    | "LlamaCpp"
    | "ik_llamacpp"
    | "Lvllm"
    | "fastllm"
    | "KTransformers"
    | "Custom"
    | "Docker";
  command: string;
  args: string[];
  env: EnvVar[];
  working_dir?: string | null;
  auto_restart: {
    enabled: boolean;
    max_retries: number;
    backoff_secs: number;
  };
  docker?: {
    container_name?: string | null;
    image_id: string;
    extra_ports?: string[];
    extra_volumes?: string[];
    extra_env?: EnvVar[];
    extra_run_args?: string[];
    gpus?: string | null;
    user?: string | null;
    command?: string | null;
    args?: string[];
    pull?: boolean | null;
    remove?: boolean | null;
  } | null;
};

export type DockerImage = {
  id: string;
  name: string;
  image: string;
  ports: string[];
  volumes: string[];
  env: EnvVar[];
  run_args: string[];
  gpus?: string | null;
  user?: string | null;
  command?: string | null;
  args: string[];
  pull: boolean;
  remove: boolean;
  build?: {
    dockerfile_content?: string | null;
    build_args?: EnvVar[];
    pull?: boolean;
    no_cache?: boolean;
  } | null;
};

export type EngineInstance = {
  id: string;
  config_id: string;
  status: string;
  pid?: number | null;
  ts?: string;
};

export type EngineItem = {
  host: Host;
  instance: EngineInstance;
  configName?: string;
};

export type EnginesResult = {
  host: Host;
  engines?: EngineInstance[];
  error?: string;
};

export type LogEntry = {
  ts: string;
  session_id: string;
  stream: string;
  line: string;
};

export type LogSession = {
  id: string;
  started_at: string;
  stopped_at?: string | null;
};

export type ModelArtifact = {
  id: string;
  kind: "snapshot" | "gguf" | string;
  path: string;
  label: string;
  library: string;
};

export type HardwareInfo = {
  hostname?: string | null;
  os_name?: string | null;
  os_version?: string | null;
  kernel_version?: string | null;
  cpu_brand?: string | null;
  cpu_cores_logical?: number | null;
  cpu_cores_physical?: number | null;
  cpu_frequency_mhz?: number | null;
  memory_total_kb?: number | null;
  memory_available_kb?: number | null;
  swap_total_kb?: number | null;
  swap_free_kb?: number | null;
  uptime_seconds?: number | null;
  gpus?: {
    name?: string | null;
    vendor?: string | null;
    memory_total_mb?: number | null;
    driver_version?: string | null;
  }[];
};

export type BenchmarkHostSnapshot = {
  id: string;
  name: string;
  base_url: string;
};

export type BenchmarkSettings = {
  model: string;
  api_base_url: string;
  pp: number[];
  tg: number[];
  depth: number[];
  runs: number;
  concurrency: number[];
  prefix_caching: boolean;
  latency_mode: string;
};

export type BenchmarkRecord = {
  id: string;
  ts: string;
  host?: BenchmarkHostSnapshot | null;
  host_hardware?: HardwareInfo | null;
  engine_config: EngineConfig;
  engine_status: string;
  settings: BenchmarkSettings;
  // Raw llama-benchy stdout (markdown table).
  output: string;
};
