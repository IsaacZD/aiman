<template>
  <div v-if="show" class="modal-backdrop">
    <div class="modal modal-wide detail-modal">
      <div class="modal-head">
        <h3>Engine detail</h3>
        <button class="ghost" @click="$emit('close')">Close</button>
      </div>
      <p class="panel-sub">
        {{ engine ? `${engine.host.name} • ${engine.instance.id}` : "Pick an engine" }}
      </p>
      <div class="history">
        <div class="history-controls">
          <label>
            Session
            <select v-model="localSelectedSessionId" :disabled="!logSessions.length">
              <option v-for="session in logSessions" :key="session.id" :value="session.id">
                {{ formatSessionLabel(session) }}
              </option>
            </select>
          </label>
          <button
            class="secondary"
            @click="$emit('select-current-session')"
            :disabled="!currentSessionId"
          >
            Current session
          </button>
          <button class="secondary" @click="$emit('refresh-sessions')" :disabled="!engine">
            Refresh sessions
          </button>
        </div>
        <div class="history-card">
          <h3>Log history</h3>
          <div class="history-list">
            <p v-if="!logHistory.length" class="empty">No log entries.</p>
            <p v-for="(item, idx) in logHistory" :key="idx">
              [{{ item.ts }}] {{ item.stream }}: {{ item.line }}
            </p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { EngineItem, LogEntry, LogSession } from "../types";
import { formatSessionLabel } from "../utils/format";

const props = defineProps<{
  show: boolean;
  engine: EngineItem | null;
  logHistory: LogEntry[];
  logSessions: LogSession[];
  selectedSessionId: string | null;
  currentSessionId: string | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "select-current-session"): void;
  (e: "refresh-sessions"): void;
  (e: "update:selectedSessionId", value: string | null): void;
}>();

const localSelectedSessionId = computed({
  get: () => props.selectedSessionId,
  set: (value) => emit("update:selectedSessionId", value)
});
</script>
