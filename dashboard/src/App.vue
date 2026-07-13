<script setup>
import { onMounted, onBeforeUnmount, watch } from 'vue'
import { useUiStore } from './stores/ui.js'
import { useMemoriesStore } from './stores/memories.js'
import { useGraphStore } from './stores/graph.js'
import { useFeedbackStore } from './stores/feedback.js'
import { useTagSettingsStore } from './stores/tagSettings.js'
import { BASE } from './api/client.js'
import AppSidebar from './components/sidebar/AppSidebar.vue'
import Toast from './components/shared/Toast.vue'
import MemoriesView from './views/MemoriesView.vue'
import GraphView from './views/GraphView.vue'
import FeedbackView from './views/FeedbackView.vue'
import SettingsView from './views/SettingsView.vue'

const ui = useUiStore()
const memories = useMemoriesStore()
const graph = useGraphStore()
const fb = useFeedbackStore()
const tagSettings = useTagSettingsStore()

const VIEWS = ['memories', 'graph', 'feedback', 'settings']
const apiBase = window.HIVEMIND_API || 'http://localhost:3456'
let pollInterval
let eventSource

function applyHash() {
  const h = location.hash.replace('#/', '')
  if (VIEWS.includes(h)) ui.activeView = h
}

watch(() => ui.activeView, v => { location.hash = '#/' + v })

onMounted(async () => {
  applyHash()
  window.addEventListener('hashchange', applyHash)

  await ui.pollServerStatus()

  if (ui.serverStatus !== 'unreachable') {
    await Promise.all([
      memories.fetchAll(),
      graph.fetchEdges(),
      fb.fetchConflicts(),
      fb.fetchFeedback(),
      tagSettings.fetchNamespaces(),
    ])
  }

  pollInterval = setInterval(() => ui.pollServerStatus(), 30_000)

  if (ui.serverStatus !== 'unreachable') {
    // Silently reflects memories/edges changed elsewhere (e.g. via MCP tool
    // calls) without a visible reload — the browser reconnects on its own
    // if the connection drops.
    eventSource = new EventSource(BASE + '/api/v1/events')
    eventSource.onmessage = () => {
      memories.refreshSilently()
      graph.fetchEdges()
    }
  }
})

onBeforeUnmount(() => {
  clearInterval(pollInterval)
  eventSource?.close()
  window.removeEventListener('hashchange', applyHash)
})
</script>

<template>
  <div class="hm-shell">

    <!-- First-load loader while we confirm the backend is reachable -->
    <div v-if="ui.serverStatus === 'checking'"
      class="flex flex-col items-center justify-center w-full gap-3"
      style="color:var(--hm-text-tertiary)">
      <div class="hm-hex hm-skeleton" style="width:28px; height:28px"></div>
      <p style="font-size:12px">Connecting to HiveMind server…</p>
    </div>

    <!-- Full-screen error when server unreachable on first load -->
    <div v-else-if="ui.serverStatus === 'unreachable' && !memories.all.length"
      class="flex flex-col items-center justify-center w-full gap-4"
      style="color:var(--hm-text-secondary)">
      <p style="font-size:14px">Cannot connect to HiveMind server at
        <code class="font-mono">{{ apiBase }}</code>.</p>
      <p style="font-size:12px; color:var(--hm-text-tertiary)">Run <code class="font-mono">hivemind up</code> and then retry.</p>
      <button class="hm-btn hm-btn-default mt-2" @click="ui.pollServerStatus().then(() => memories.fetchAll())">
        Retry
      </button>
    </div>

    <template v-else>
      <AppSidebar />
      <main class="flex flex-1 overflow-hidden">
        <MemoriesView v-show="ui.activeView === 'memories'" />
        <GraphView v-show="ui.activeView === 'graph'" />
        <FeedbackView v-show="ui.activeView === 'feedback'" />
        <SettingsView v-show="ui.activeView === 'settings'" />
      </main>
    </template>

    <Toast />
  </div>
</template>
