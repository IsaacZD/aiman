import { ref, computed } from "vue";
import ReconnectingWebSocket from "partysocket/ws";
import type { EngineItem, LogEntry, LogSession } from "../types";

export function useLogs() {
  const logs = ref<string[]>([]);
  const logHistory = ref<LogEntry[]>([]);
  const logSessions = ref<LogSession[]>([]);
  const selectedSessionId = ref<string | null>(null);

  let ws: ReconnectingWebSocket | null = null;
  let wsUserClosed = false;
  let historyLoadTimer: number | null = null;
  let historyRequestId = 0;
  let activeAbortController: AbortController | null = null;

  function getAbortSignal(): AbortSignal {
    activeAbortController?.abort();
    activeAbortController = new AbortController();
    return activeAbortController.signal;
  }

  const currentSessionId = computed(() => {
    const running = logSessions.value.find((session) => !session.stopped_at);
    return running?.id ?? null;
  });

  function connectLogs(engine: EngineItem) {
    if (ws) {
      wsUserClosed = true;
      ws.close();
    }

    wsUserClosed = false;
    logs.value = [];
    const { host, instance } = engine;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    ws = new ReconnectingWebSocket(
      `${protocol}//${window.location.host}/api/hosts/${host.id}/engines/${instance.id}/logs/ws`,
      [],
      { maxRetries: 10 }
    );

    ws.onmessage = (event) => {
      try {
        const entry = JSON.parse(event.data as string);
        logs.value.push(`[${entry.ts}] ${entry.stream}: ${entry.line}`);
        if (logs.value.length > 500) {
          logs.value.shift();
        }
      } catch {
        logs.value.push(event.data as string);
      }
    };
  }

  function disconnectLogs() {
    if (ws) {
      wsUserClosed = true;
      ws.close();
      ws = null;
    }
  }

  /// Called when an engine_status SSE event arrives for the currently-viewed engine.
  /// Triggers a one-shot reload of sessions + history (history reload is scheduled
  /// inside loadLogSessions once the session list settles).
  function notifyEngineStatusChanged(engine: EngineItem | null) {
    if (!engine) return;
    void loadLogSessions(engine);
  }

  function scheduleLogHistoryLoad(loadFn: () => Promise<void>) {
    if (historyLoadTimer !== null) {
      window.clearTimeout(historyLoadTimer);
    }
    historyLoadTimer = window.setTimeout(() => {
      historyLoadTimer = null;
      void loadFn();
    }, 150);
  }

  function deferUiUpdate(task: () => void) {
    const idle = (globalThis as any).requestIdleCallback as
      | ((cb: () => void, options?: { timeout: number }) => number)
      | undefined;
    if (idle) {
      idle(() => task(), { timeout: 200 });
    } else {
      window.setTimeout(task, 0);
    }
  }

  async function loadLogHistory(engine: EngineItem | null) {
    if (!engine) {
      return;
    }
    if (!selectedSessionId.value) {
      logHistory.value = [];
      return;
    }

    const requestId = ++historyRequestId;
    const signal = getAbortSignal();
    const { host, instance } = engine;
    let logsRes: Response;
    try {
      logsRes = await fetch(
        `/api/hosts/${host.id}/engines/${instance.id}/logs?session_id=${encodeURIComponent(
          selectedSessionId.value
        )}&limit=1000`,
        { signal }
      );
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") return;
      throw err;
    }

    if (requestId !== historyRequestId) {
      return;
    }

    if (logsRes.ok) {
      const body = (await logsRes.json()) as { entries: LogEntry[] };
      deferUiUpdate(() => {
        logHistory.value = body.entries ?? [];
      });
    }
  }

  async function loadLogSessions(engine: EngineItem | null) {
    if (!engine) {
      return;
    }

    const requestId = ++historyRequestId;
    const signal = getAbortSignal();
    const { host, instance } = engine;
    let res: Response;
    try {
      res = await fetch(
        `/api/hosts/${host.id}/engines/${instance.id}/logs/sessions?limit=50`,
        { signal }
      );
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") return;
      throw err;
    }

    if (requestId !== historyRequestId) {
      return;
    }

    if (res.ok) {
      const body = (await res.json()) as { sessions: LogSession[] };
      deferUiUpdate(() => {
        logSessions.value = body.sessions ?? [];
        if (!logSessions.value.length) {
          selectedSessionId.value = null;
          logHistory.value = [];
          return;
        }
        const nextId = logSessions.value[0]?.id ?? null;
        if (
          !selectedSessionId.value ||
          !logSessions.value.some((s) => s.id === selectedSessionId.value)
        ) {
          selectedSessionId.value = nextId;
        }
        scheduleLogHistoryLoad(() => loadLogHistory(engine));
      });
    }
  }

  function selectCurrentSession() {
    if (!currentSessionId.value) {
      return;
    }
    selectedSessionId.value = currentSessionId.value;
  }

  function clearLogsState() {
    if (historyLoadTimer !== null) {
      window.clearTimeout(historyLoadTimer);
      historyLoadTimer = null;
    }
    activeAbortController?.abort();
    activeAbortController = null;
    logs.value = [];
    logHistory.value = [];
    logSessions.value = [];
    selectedSessionId.value = null;
  }

  return {
    logs,
    logHistory,
    logSessions,
    selectedSessionId,
    currentSessionId,
    connectLogs,
    disconnectLogs,
    notifyEngineStatusChanged,
    scheduleLogHistoryLoad,
    loadLogHistory,
    loadLogSessions,
    selectCurrentSession,
    clearLogsState
  };
}
