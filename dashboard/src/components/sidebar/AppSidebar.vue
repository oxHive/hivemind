<script setup>
import { computed } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'
import { useGraphStore } from '../../stores/graph.js'
import { useSuggestStore } from '../../stores/suggest.js'
import StatusRow from './StatusRow.vue'
import oxhiveMark from '../../assets/oxhive-mark.png'

const ui = useUiStore()
const feedback = useFeedbackStore()
const graph = useGraphStore()
const suggest = useSuggestStore()

function toggleSuggestions() {
  if (suggest.panelOpen) suggest.closePanel()
  else suggest.openPanel()
}

const feedbackCount = computed(() =>
  feedback.conflicts.length + feedback.feedbackItems.length
)

const navItems = [
  { id: 'analytics', label: 'Analytics' },
  { id: 'memories', label: 'Memories' },
  { id: 'graph', label: 'Graph' },
  { id: 'feedback', label: 'Feedback' },
  { id: 'settings', label: 'Settings' },
]

const statusDot = computed(() => {
  if (ui.serverStatus === 'unreachable') return 'red'
  if (ui.serverStatus === 'sync_failed') return 'red'
  if (ui.serverStatus === 'syncing') return 'amber'
  return 'green'
})

const statusText = computed(() => {
  if (ui.serverStatus === 'unreachable') return 'unreachable'
  if (ui.serverStatus === 'syncing') return 'syncing…'
  if (ui.serverStatus === 'sync_failed') return 'sync failed'
  return 'running'
})

const memoryCount = computed(() => ui.serverInfo?.memory_count ?? ui.serverInfo?.memoryCount ?? '—')

const syncInfo = computed(() => ui.syncInfo)

const syncStatusText = computed(() => {
  if (!syncInfo.value?.enabled) return null
  const last = syncInfo.value?.last_synced_at
  if (!last) return 'not yet synced'
  const diffSec = Math.floor(Date.now() / 1000) - last
  if (diffSec < 60) return 'synced · just now'
  const diffMin = Math.floor(diffSec / 60)
  return `synced · ${diffMin}m ago`
})

const syncDot = computed(() => {
  if (!syncInfo.value?.enabled) return null
  const last = syncInfo.value?.last_synced_at
  if (!last) return 'gray'
  const diffSec = Math.floor(Date.now() / 1000) - last
  return diffSec > 600 ? 'amber' : 'green'
})
</script>

<template>
  <nav class="flex flex-col shrink-0 h-full"
    style="width:200px; background:var(--hm-bg-surface); border-right:0.5px solid var(--hm-border-subtle)">

    <!-- Logo -->
    <div class="px-5 pt-6 pb-7 flex items-center justify-start gap-2"
      style="border-bottom:0.5px solid var(--hm-border-subtle)">
      <div class="flex items-center" style="gap:4px">
        <svg width="24" height="24" viewBox="0 0 16 16" aria-hidden="true">
          <polygon points="8,1.5 13.6,4.75 13.6,11.25 8,14.5 2.4,11.25 2.4,4.75"
            fill="none" stroke="var(--hm-accent)" stroke-width="1.2" />
          <circle cx="8" cy="8" r="2" fill="var(--hm-accent)" />
        </svg>
        <div style="font-size:19px; font-weight:600; letter-spacing:-0.01em; color:var(--hm-text-primary); line-height:1">HiveMind</div>
      </div>
      <span class="font-mono self-end" style="font-size:10px; color:var(--hm-text-tertiary); line-height:1">
        v{{ ui.serverInfo?.version || '—' }}
      </span>
    </div>

    <!-- Nav -->
    <ul class="flex flex-col py-3">
      <li v-for="item in navItems" :key="item.id">
        <button
          @click="ui.activeView = item.id"
          class="nav-item"
          :class="{ 'nav-item--active': ui.activeView === item.id }"
          :aria-current="ui.activeView === item.id ? 'page' : undefined"
        >
          <span>{{ item.label }}</span>
          <span v-if="item.id === 'feedback' && feedbackCount > 0"
            class="font-mono rounded-sm px-1.5 py-0.5"
            style="font-size:10px; background:var(--hm-warning-bg); color:var(--hm-warning)">
            {{ feedbackCount }}
          </span>
        </button>
      </li>
    </ul>

    <!-- Suggestions toggle: visible from any page whenever there are
         pending connection suggestions to review. -->
    <button v-if="graph.pendingEdges.length" class="suggest-toggle mt-auto"
      :class="{ 'suggest-toggle--active': suggest.panelOpen }"
      :aria-pressed="suggest.panelOpen" @click="toggleSuggestions">
      <span>✦ Suggestions</span>
      <span class="font-mono rounded-sm px-1.5 py-0.5"
        style="font-size:10px; background:var(--hm-warning-bg); color:var(--hm-warning)">
        {{ graph.pendingEdges.length }}
      </span>
    </button>

    <!-- Status (push to bottom) -->
    <div :class="graph.pendingEdges.length ? '' : 'mt-auto'" class="px-5 pb-5 pt-4"
      style="border-top:0.5px solid var(--hm-border-subtle)">
      <StatusRow v-if="syncStatusText" :dot="syncDot" :text="syncStatusText" />
      <StatusRow
        v-if="(syncInfo?.conflict_count ?? 0) > 0"
        dot="amber"
        :text="`${syncInfo.conflict_count} conflict${syncInfo.conflict_count > 1 ? 's' : ''} need review`"
        :class="{ 'mt-1': syncStatusText }"
      />
      <StatusRow :dot="statusDot" :text="statusText" :class="{ 'mt-1': syncStatusText || (syncInfo?.conflict_count ?? 0) > 0 }" />
      <StatusRow dot="gray" :text="`${memoryCount} memories`" class="mt-1" />
    </div>

    <!-- Footer -->
    <div class="footer">
      <img class="footer__mark" :src="oxhiveMark" alt="" aria-hidden="true" width="18" height="18" />
      <span class="footer__word">OxHive</span>
    </div>
  </nav>
</template>

<style scoped>
.nav-item {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 20px;
  font-size: 13px;
  text-align: left;
  color: var(--hm-text-secondary);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.nav-item:hover,
.nav-item:focus-visible {
  background: var(--hm-bg-elevated);
  color: var(--hm-text-primary);
  outline: none;
}

.nav-item:focus-visible {
  outline: 2px solid var(--hm-accent);
  outline-offset: -2px;
}

.nav-item--active {
  background: var(--hm-bg-elevated);
  color: var(--hm-text-primary);
  font-weight: 500;
  box-shadow: inset 2px 0 0 var(--hm-accent);
}

.suggest-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 0 12px;
  padding: 8px 8px;
  font-size: 12px;
  color: var(--hm-warning);
  text-align: left;
  background: transparent;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.1s;
}

.suggest-toggle:hover,
.suggest-toggle:focus-visible {
  background: var(--hm-warning-bg);
  outline: none;
}

.suggest-toggle--active {
  background: var(--hm-warning-bg);
}

.footer {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 14px 20px 16px;
}

.footer__mark {
  display: block;
  filter: brightness(0) invert(1);
}

:root[data-theme="light"] .footer__mark {
  filter: none;
}

.footer__word {
  font-family: "Hanken Grotesk", var(--hm-font-sans);
  font-size: 15px;
  font-weight: 800;
  letter-spacing: -0.02em;
  color: var(--hm-text-primary);
}
</style>
