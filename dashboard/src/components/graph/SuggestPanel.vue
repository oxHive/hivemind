<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { useGraphStore } from '../../stores/graph.js'
import { useMemoriesStore } from '../../stores/memories.js'
import { useSuggestStore } from '../../stores/suggest.js'

const graph = useGraphStore()
const memories = useMemoriesStore()
const suggest = useSuggestStore()

const revisingFor = ref(null)   // edge id whose revise textbox is open
const feedbackText = ref('')

// Ticking elapsed-time display while the agent is analyzing. Starts on
// entering 'suggesting', stops the instant the SSE moves the phase along
// (e.g. 'suggestions_ready') so the clock never overruns the real work.
const now = ref(Date.now())
let tickTimer = null
watch(() => suggest.phase, (phase) => {
  if (phase === 'suggesting') {
    now.value = Date.now()
    if (!tickTimer) tickTimer = setInterval(() => { now.value = Date.now() }, 1000)
  } else if (tickTimer) {
    clearInterval(tickTimer)
    tickTimer = null
  }
}, { immediate: true })
onBeforeUnmount(() => { if (tickTimer) clearInterval(tickTimer) })

const elapsedSeconds = computed(() => {
  if (!suggest.suggestingStartedAt) return 0
  return Math.max(0, Math.floor((now.value - suggest.suggestingStartedAt) / 1000))
})

const rows = computed(() => graph.pendingEdges.map(e => ({
  ...e,
  sourceTitle: memories.all.find(m => m.id === e.source_id)?.title ?? e.source_id,
  targetTitle: memories.all.find(m => m.id === e.target_id)?.title ?? e.target_id,
})))

function rowState(edge) {
  if (suggest.revisingEdgeId === edge.id) return 'revising'
  if (suggest.queuedEdgeIds.includes(edge.id)) return 'queued'
  return 'idle'
}

// Selecting a suggestion shows the source memory's detail (in whichever
// view is mounted — Graph's DetailPanel or the Memories page) and, on the
// Graph page, highlights the pending edge on the canvas via selectedEdgeId.
// The preview of what Approve will change now renders in the content field
// itself (MemoryDetail/DetailPanel), not inline here.
function selectRow(edge) {
  const isSame = graph.selectedEdgeId === edge.id
  graph.selectedEdgeId = isSame ? null : edge.id
  if (isSame) return
  graph.selectedNodeId = edge.source_id
  const source = memories.all.find(m => m.id === edge.source_id)
  if (source) memories.select(source)
}

function openRevise(edge) {
  revisingFor.value = edge.id
  feedbackText.value = ''
}

function submitRevise(edge) {
  const text = feedbackText.value.trim()
  if (!text) return
  suggest.revise(edge.id, text)
  revisingFor.value = null
  feedbackText.value = ''
}

function close() {
  suggest.closePanel()
  graph.selectedEdgeId = null
}

async function endSession() {
  await suggest.end()
}
</script>

<template>
  <div class="flex flex-col h-full shrink-0"
    style="width:300px; background:var(--hm-bg-surface); border-left:0.5px solid var(--hm-border-subtle)">

    <div class="flex items-center justify-between px-4"
      style="height:52px; border-bottom:0.5px solid var(--hm-border-subtle)">
      <span style="font-size:13px; font-weight:500; color:var(--hm-text-primary)">
        ✦ Suggestions
      </span>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="close">✕</button>
    </div>

    <div v-if="suggest.error" class="px-4 py-2"
      style="font-size:11px; color:var(--hm-warning); background:var(--hm-warning-bg)">
      {{ suggest.error }}
    </div>

    <div v-if="suggest.phase === 'suggesting'" class="px-4 py-3 flex items-center gap-2"
      style="font-size:13px; font-weight:600; color:var(--hm-text-primary)">
      <span class="hm-skeleton" style="display:inline-block; width:10px; height:10px; border-radius:50%"></span>
      Agent is analyzing your memories…
      <span class="font-mono" style="margin-left:auto; font-weight:500; font-size:11px; color:var(--hm-text-tertiary)">
        {{ elapsedSeconds }}s
      </span>
    </div>

    <div class="flex-1 overflow-y-auto">
      <p v-if="!rows.length && suggest.phase !== 'suggesting'" class="px-4 py-3"
        style="font-size:12px; color:var(--hm-text-tertiary)">
        No pending suggestions.
      </p>

      <div v-for="edge in rows" :key="edge.id" class="px-4 py-3 cursor-pointer"
        :style="{ borderBottom: '0.5px solid var(--hm-border-subtle)',
                  background: graph.selectedEdgeId === edge.id ? 'var(--hm-warning-bg)' : 'transparent' }"
        @click="selectRow(edge)">

        <p style="font-size:12px; color:var(--hm-text-primary)">
          {{ edge.sourceTitle }}
          <span class="font-mono" style="font-size:10px; color:var(--hm-warning)">
            --[{{ edge.relationship }}]--></span>
          {{ edge.targetTitle }}
        </p>
        <p v-if="edge.reason" class="mt-1" style="font-size:11px; color:var(--hm-text-secondary)">
          {{ edge.reason }}
        </p>

        <div v-if="rowState(edge) === 'revising'" class="mt-2 flex items-center gap-1.5"
          style="font-size:11px; color:var(--hm-warning)">
          <span class="hm-skeleton" style="display:inline-block; width:8px; height:8px; border-radius:50%"></span>
          revising…
        </div>
        <div v-else-if="rowState(edge) === 'queued'" class="mt-2"
          style="font-size:11px; color:var(--hm-text-tertiary)">
          queued
        </div>

        <div v-else class="mt-2 flex gap-1.5" @click.stop>
          <button class="hm-btn hm-btn-primary hm-btn-sm" @click="graph.resolveEdge(edge.id, 'active')">Approve</button>
          <button class="hm-btn hm-btn-default hm-btn-sm" @click="graph.resolveEdge(edge.id, 'rejected')">Reject</button>
          <button v-if="suggest.active" class="hm-btn hm-btn-ghost hm-btn-sm" @click="openRevise(edge)">Revise</button>
        </div>

        <input v-if="revisingFor === edge.id" v-model="feedbackText"
          class="hm-input mt-2 w-full" placeholder="What should change? Enter to send"
          @click.stop @keyup.enter="submitRevise(edge)" @keyup.esc="revisingFor = null" />
      </div>
    </div>

    <div class="px-4 flex items-center gap-1.5" style="height:40px; border-top:0.5px solid var(--hm-border-subtle)">
      <template v-if="rows.length">
        <button class="hm-btn hm-btn-primary hm-btn-sm" @click="graph.acceptAllPending()">Accept all</button>
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="graph.rejectAllPending()">Reject all</button>
      </template>
      <button v-if="suggest.active" class="hm-btn hm-btn-default hm-btn-sm ml-auto" @click="endSession">
        End session
      </button>
    </div>
  </div>
</template>
