export type HostConfig = {
  id: string;
  name: string;
  base_url: string;
  api_key?: string;
  model_libraries?: string[];
};

export type HostsFile = {
  host?: HostConfig[];
};

export type EngineConfig = {
  id: string;
  name: string;
  // Keep in sync with crates/shared EngineType (string values serialized over the wire).
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
  env: { key: string; value: string }[];
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
    extra_env?: { key: string; value: string }[];
    extra_run_args?: string[];
    workdir?: string | null;
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
  env: { key: string; value: string }[];
  run_args: string[];
  workdir?: string | null;
  user?: string | null;
  command?: string | null;
  args: string[];
  pull: boolean;
  remove: boolean;
  build?: {
    context?: string | null;
    dockerfile?: string | null;
    dockerfile_content?: string | null;
    target?: string | null;
    build_args?: { key: string; value: string }[];
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

export type BenchmarkSettings = {
  concurrency: number[];
  requests_per_concurrency: number;
  prompt: string;
  prompt_words: number;
  max_tokens: number;
  temperature: number;
  model: string;
  api_base_url: string;
  timeout_seconds: number;
};

export type BenchmarkResult = {
  concurrency: number;
  requests: number;
  success_count: number;
  error_count: number;
  duration_ms: number;
  avg_latency_ms: number;
  min_latency_ms: number;
  max_latency_ms: number;
  p50_latency_ms: number;
  p90_latency_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_tps: number;
  completion_tps: number;
  requests_per_sec: number;
  errors: string[];
};

export type BenchmarkRecord = {
  id: string;
  ts: string;
  origin?: "host" | "dashboard";
  host?: { id: string; name: string; base_url: string } | null;
  host_hardware?: HardwareInfo | null;
  engine_config: EngineConfig;
  engine_status: string;
  settings: BenchmarkSettings;
  results: BenchmarkResult[];
};

export type NormalizedBenchmarkSettings = {
  concurrency: number[];
  requestsPerConcurrency: number;
  prompt: string;
  promptWords: number;
  maxTokens: number;
  temperature: number;
  model: string;
  apiBaseUrl: string;
  apiKey?: string;
  timeoutSeconds: number;
};

export type RequestOutcome = {
  latency_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  error?: string;
};
