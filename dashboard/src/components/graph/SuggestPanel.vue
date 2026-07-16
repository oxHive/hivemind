<script setup>
import { ref, computed } from 'vue'
import { useGraphStore } from '../../stores/graph.js'
import { useMemoriesStore } from '../../stores/memories.js'
import { useSuggestStore } from '../../stores/suggest.js'

const graph = useGraphStore()
const memories = useMemoriesStore()
const suggest = useSuggestStore()

const revisingFor = ref(null)   // edge id whose revise textbox is open
const feedbackText = ref('')

const rows = computed(() => graph.pendingEdges.map(e => ({
  ...e,
  sourceTitle: memories.all.find(m => m.id === e.source_id)?.title ?? e.source_id,
  targetTitle: memories.all.find(m => m.id === e.target_id)?.title ?? e.target_id,
})))

function titleCase(s) { return s.charAt(0).toUpperCase() + s.slice(1) }

function rowState(edge) {
  if (suggest.revisingEdgeId === edge.id) return 'revising'
  if (suggest.queuedEdgeIds.includes(edge.id)) return 'queued'
  return 'idle'
}

function selectRow(edge) {
  graph.selectedEdgeId = graph.selectedEdgeId === edge.id ? null : edge.id
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

    <div class="flex items-center justify-between px-4 py-2.5"
      style="border-bottom:0.5px solid var(--hm-border-subtle)">
      <span style="font-size:13px; font-weight:500; color:var(--hm-text-primary)">
        ✦ Suggestions
      </span>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="close">✕</button>
    </div>

    <div v-if="suggest.error" class="px-4 py-2"
      style="font-size:11px; color:var(--hm-warning); background:var(--hm-warning-bg)">
      {{ suggest.error }}
    </div>

    <div v-if="suggest.phase === 'suggesting'" class="px-4 py-3"
      style="font-size:12px; color:var(--hm-text-tertiary)">
      <span class="hm-skeleton" style="display:inline-block; width:10px; height:10px; border-radius:50%"></span>
      Agent is analyzing your memories…
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

    <div class="px-4 py-2.5 flex gap-1.5" style="border-top:0.5px solid var(--hm-border-subtle)">
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
