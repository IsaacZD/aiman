<template>
  <main class="page">
    <header class="hero">
      <div>
        <p class="eyebrow">aiman dashboard</p>
        <h1>LLM Engines at a glance</h1>
        <p class="subtext">
          Start, stop, and monitor your vLLM, llama.cpp, and ktransformers engines across
          your LAN.
        </p>
      </div>
      <div class="status-card">
        <p class="status-label">Connected hosts</p>
        <p class="status-value">0</p>
        <p class="status-hint">Configure hosts in configs/hosts.toml</p>
      </div>
    </header>

    <section class="panel">
      <div class="panel-head">
        <h2>Engines</h2>
        <button class="primary">Refresh</button>
      </div>
      <div class="empty">
        <p>No engines loaded yet.</p>
        <p>When the dashboard server connects to a host, running engines will appear here.</p>
      </div>
    </section>
  </main>
</template>

<script setup lang="ts"></script>
