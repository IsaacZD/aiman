<template>
  <div class="hardware-card">
    <p class="hardware-title">Hardware</p>
    <template v-if="hardware">
      <div class="hardware-grid">
        <div class="hardware-item">
          <span class="hardware-label">CPU</span>
          <span class="hardware-value">{{ formatCpu(hardware) }}</span>
        </div>
        <div class="hardware-item">
          <span class="hardware-label">Memory</span>
          <span class="hardware-value">{{ formatMemory(hardware) }}</span>
        </div>
        <div class="hardware-item">
          <span class="hardware-label">OS</span>
          <span class="hardware-value">{{ formatOs(hardware) }}</span>
        </div>
        <div class="hardware-item">
          <span class="hardware-label">GPU</span>
          <span class="hardware-value">{{ formatGpus(hardware) }}</span>
        </div>
        <div class="hardware-item">
          <span class="hardware-label">Uptime</span>
          <span class="hardware-value">{{ formatUptime(hardware) }}</span>
        </div>
      </div>
    </template>
    <p v-else class="hardware-empty">No hardware data.</p>
  </div>
</template>

<script setup lang="ts">
import type { HardwareInfo } from "../types";
import { formatCpu, formatMemory, formatOs, formatGpus, formatUptime } from "../utils/format";

defineProps<{
  hardware: HardwareInfo | null | undefined;
}>();
</script>
