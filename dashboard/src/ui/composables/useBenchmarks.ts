import { ref } from "vue";
import type { Host, EngineItem, BenchmarkRecord } from "../types";

export function createBenchmarkForm() {
  return {
    pp: "512,2048",
    tg: "32,128",
    depth: "0",
    runs: 3,
    concurrency: "1",
    model: "",
    apiBaseUrl: "",
    apiKey: "",
    prefixCaching: false,
    latencyMode: "generation" as "api" | "generation" | "none",
    noWarmup: false
  };
}

export function useBenchmarks() {
  const benchmarkRecords = ref<BenchmarkRecord[]>([]);
  const benchmarkErrors = ref<string[]>([]);
  const benchmarkLoading = ref(false);
  const showBenchmarkModal = ref(false);
  const benchmarkTarget = ref<EngineItem | null>(null);
  const benchmarkForm = ref(createBenchmarkForm());
  const benchmarkModalError = ref<string | null>(null);
  const benchmarkRunning = ref(false);

  function openBenchmarkModal(engine: EngineItem) {
    benchmarkTarget.value = engine;
    benchmarkForm.value = createBenchmarkForm();
    benchmarkModalError.value = null;
    showBenchmarkModal.value = true;
  }

  function closeBenchmarkModal() {
    showBenchmarkModal.value = false;
    benchmarkModalError.value = null;
    benchmarkRunning.value = false;
  }

  async function loadBenchmarks() {
    benchmarkLoading.value = true;
    benchmarkErrors.value = [];
    try {
      const res = await fetch("/api/benchmarks");
      if (!res.ok) {
        benchmarkErrors.value = [`Failed to load benchmarks (HTTP ${res.status})`];
        benchmarkRecords.value = [];
        return;
      }
      const body = (await res.json()) as {
        results: { host: Host; records?: BenchmarkRecord[]; error?: string }[];
        local?: BenchmarkRecord[];
      };
      const next: BenchmarkRecord[] = [];
      const errors: string[] = [];
      for (const result of body.results ?? []) {
        if (result.error) {
          errors.push(`${result.host.name}: ${result.error}`);
          continue;
        }
        for (const record of result.records ?? []) {
          next.push(record);
        }
      }
      for (const record of body.local ?? []) {
        next.push(record);
      }
      next.sort((a, b) => (a.ts < b.ts ? 1 : -1));
      benchmarkRecords.value = next;
      benchmarkErrors.value = errors;
    } catch (err) {
      benchmarkErrors.value = [(err as Error).message];
      benchmarkRecords.value = [];
    } finally {
      benchmarkLoading.value = false;
    }
  }

  async function runBenchmark() {
    if (!benchmarkTarget.value) {
      return;
    }
    benchmarkModalError.value = null;

    const f = benchmarkForm.value;
    const payload = {
      pp: f.pp,
      tg: f.tg,
      depth: f.depth,
      runs: f.runs,
      concurrency: f.concurrency,
      model: f.model.trim() || undefined,
      api_base_url: f.apiBaseUrl.trim() || undefined,
      api_key: f.apiKey.trim() || undefined,
      prefix_caching: f.prefixCaching,
      latency_mode: f.latencyMode,
      no_warmup: f.noWarmup
    };

    benchmarkRunning.value = true;
    try {
      const res = await fetch(
        `/api/hosts/${benchmarkTarget.value.host.id}/engines/${benchmarkTarget.value.instance.id}/benchmark`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload)
        }
      );
      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as { error?: string } | null;
        benchmarkModalError.value = body?.error
          ? `Benchmark failed: ${body.error}`
          : `Benchmark failed (HTTP ${res.status}).`;
        return;
      }
      closeBenchmarkModal();
      await loadBenchmarks();
    } catch (err) {
      benchmarkModalError.value = (err as Error).message;
    } finally {
      benchmarkRunning.value = false;
    }
  }

  return {
    benchmarkRecords,
    benchmarkErrors,
    benchmarkLoading,
    showBenchmarkModal,
    benchmarkTarget,
    benchmarkForm,
    benchmarkModalError,
    benchmarkRunning,
    openBenchmarkModal,
    closeBenchmarkModal,
    loadBenchmarks,
    runBenchmark
  };
}
