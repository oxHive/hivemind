<script setup>
import { computed } from 'vue'
import { AppSidebar as OxAppSidebar, AppNav } from '@oxhive/ui'
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'
import { useGraphStore } from '../../stores/graph.js'
import { useUpdateStore } from '../../stores/update.js'
import { useNavItems } from '../../config/nav.js'
import StatusRow from './StatusRow.vue'
import { BASE } from '../../api/client.js'

const ui = useUiStore()
const feedback = useFeedbackStore()
const graph = useGraphStore()
const update = useUpdateStore()
const navItems = useNavItems()

const statusDot = computed(() => {
  if (ui.serverStatus === 'unreachable') return 'red'
  if (ui.serverStatus === 'sync_failed') return 'red'
  if (ui.serverStatus === 'syncing') return 'amber'
  return 'green'
})

const memoryCount = computed(() => {
  if (!ui.serverInfo) return '—'
  const primary = ui.serverInfo?.memory_count ?? ui.serverInfo?.memoryCount ?? 0
  const org = ui.orgInfo?.count ?? 0
  return primary + org
})

const serverAddress = computed(() => (BASE || 'http://localhost:3456').replace(/^https?:\/\//, ''))

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

const conflictCount = computed(() => syncInfo.value?.conflict_count ?? 0)
const conflictDot = computed(() => (conflictCount.value > 0 ? 'amber' : 'green'))

const orgInfo = computed(() => ui.orgInfo)

const orgSyncStatusText = computed(() => {
  if (!orgInfo.value?.configured) return null
  if (!orgInfo.value?.enabled) return 'configured, sync disabled'
  const last = orgInfo.value?.last_synced_at
  if (!last) return 'not yet synced'
  const diffSec = Math.floor(Date.now() / 1000) - last
  if (diffSec < 60) return 'synced · just now'
  const diffMin = Math.floor(diffSec / 60)
  return `synced · ${diffMin}m ago`
})

const orgSyncDot = computed(() => {
  if (!orgInfo.value?.configured || !orgInfo.value?.enabled) return 'gray'
  const last = orgInfo.value?.last_synced_at
  if (!last) return 'gray'
  const diffSec = Math.floor(Date.now() / 1000) - last
  return diffSec > 600 ? 'amber' : 'green'
})
</script>

<template>
  <OxAppSidebar product-name="HiveMind" :version="ui.serverInfo?.version || ''">
    <template #logo-icon>
      <svg width="24" height="24" viewBox="0 0 16 16" aria-hidden="true">
        <polygon points="8,1.5 13.6,4.75 13.6,11.25 8,14.5 2.4,11.25 2.4,4.75"
          fill="none" stroke="var(--hm-accent)" stroke-width="1.2" />
        <circle cx="8" cy="8" r="2" fill="var(--hm-accent)" />
      </svg>
    </template>

    <AppNav :items="navItems" />

    <template #status>
      <div v-if="update.available && update.platformSupported" class="px-2 pb-2">
        <button @click="update.changelogOpen = true" class="oxui-nav-item" style="border-radius:6px; flex-wrap:nowrap">
          <span style="white-space:nowrap; overflow:hidden; text-overflow:ellipsis; min-width:0">Update available</span>
          <span class="font-mono rounded-sm px-1.5 py-0.5"
            style="font-size:10px; background:var(--hm-warning-bg); color:var(--hm-warning); white-space:nowrap; flex-shrink:0">
            v{{ update.latestVersion }}
          </span>
        </button>
      </div>
    </template>

    <template #footer>
      <StatusRow :dot="statusDot" k="server" :v="serverAddress" />
      <StatusRow v-if="syncStatusText" :dot="syncDot" pulse k="sync" :v="syncStatusText" class="mt-1" />
      <StatusRow v-if="orgInfo?.configured" :dot="orgSyncDot" pulse k="org" :v="orgSyncStatusText" class="mt-1" />
      <StatusRow :dot="conflictDot" k="conflicts" :v="String(conflictCount)" class="mt-1" />
      <StatusRow dot="gray" :text="`${memoryCount} memories`" class="mt-1" />
    </template>
  </OxAppSidebar>
</template>
