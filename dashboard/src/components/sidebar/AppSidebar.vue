<script setup>
import { computed } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'
import StatusRow from './StatusRow.vue'

const ui = useUiStore()
const feedback = useFeedbackStore()

const feedbackCount = computed(() =>
  feedback.conflicts.length + feedback.feedbackItems.length
)

const navItems = [
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
    <div class="px-4 pt-5 pb-4" style="border-bottom:0.5px solid var(--hm-border-subtle)">
      <div style="font-size:15px; font-weight:500; color:var(--hm-text-primary)">HiveMind</div>
      <div class="font-mono mt-0.5" style="font-size:10px; color:var(--hm-text-tertiary)">
        v{{ ui.serverInfo?.version || '—' }}
      </div>
    </div>

    <!-- Nav -->
    <ul class="flex flex-col py-2">
      <li v-for="item in navItems" :key="item.id">
        <button
          @click="ui.activeView = item.id"
          class="w-full flex items-center justify-between px-4 py-2 text-sm text-left"
          :style="ui.activeView === item.id
            ? 'background:var(--hm-bg-elevated); color:var(--hm-text-primary); font-weight:500; border-right:2px solid var(--hm-personal)'
            : 'color:var(--hm-text-secondary); border-right:2px solid transparent'"
          style="transition:background 0.1s"
          @mouseover="$event.currentTarget.style.background = ui.activeView !== item.id ? 'var(--hm-bg-elevated)' : ''"
          @mouseleave="$event.currentTarget.style.background = ui.activeView !== item.id ? '' : ''"
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

    <!-- Status (push to bottom) -->
    <div class="mt-auto px-4 pb-4 pt-3" style="border-top:0.5px solid var(--hm-border-subtle)">
      <StatusRow :dot="statusDot" :text="statusText" />
      <StatusRow dot="gray" :text="`${memoryCount} memories`" class="mt-1" />
      <StatusRow v-if="syncStatusText" :dot="syncDot" :text="syncStatusText" class="mt-1" />
      <StatusRow
        v-if="(syncInfo?.conflict_count ?? 0) > 0"
        dot="amber"
        :text="`${syncInfo.conflict_count} conflict${syncInfo.conflict_count > 1 ? 's' : ''} need review`"
        class="mt-1"
      />
    </div>
  </nav>
</template>
