<script setup>
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'
import { useMemoriesStore } from '../../stores/memories.js'

const props = defineProps({ item: Object })
const ui = useUiStore()
const fb = useFeedbackStore()
const memories = useMemoriesStore()

const typeColors = {
  outdated: { bg: 'var(--hm-warning-bg)', text: 'var(--hm-warning)', border: 'var(--hm-warning-border)' },
  incorrect: { bg: 'var(--hm-danger-bg)', text: 'var(--hm-danger)', border: 'var(--hm-danger-border)' },
  duplicate: { bg: 'var(--hm-bg-overlay)', text: 'var(--hm-text-secondary)', border: 'var(--hm-border-default)' },
  other: { bg: 'var(--hm-bg-overlay)', text: 'var(--hm-text-secondary)', border: 'var(--hm-border-default)' },
}

function navToMemory() {
  const mem = memories.all.find(m => m.id === props.item.memory_id)
  if (mem) {
    memories.select(mem)
    ui.activeView = 'memories'
  }
}
</script>

<template>
  <div class="rounded-lg p-5 mb-4"
    style="background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-default)">
    <div class="flex items-start justify-between mb-3">
      <div class="flex items-center gap-2">
        <span class="font-mono rounded-sm px-1.5 py-0.5"
          :style="`font-size:9px; background:${(typeColors[item.signal]||typeColors.other).bg}; color:${(typeColors[item.signal]||typeColors.other).text}; border:0.5px solid ${(typeColors[item.signal]||typeColors.other).border}`">
          {{ item.signal || 'other' }}
        </span>
        <button @click="navToMemory"
          style="font-size:13px; font-weight:500; color:var(--hm-text-primary); cursor:pointer; background:none; border:none; text-align:left">
          {{ item.title || item.memory_id }}
        </button>
      </div>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" aria-label="Dismiss feedback" @click="fb.dismissFeedback(item.id)">✕</button>
    </div>

    <p v-if="item.note" style="font-size:12px; color:var(--hm-text-secondary)" class="mb-4">{{ item.note }}</p>

    <button class="hm-btn hm-btn-default hm-btn-sm font-mono"
      @click="ui.copyToClipboard(`/memory-edit ${item.memory_id}`)">
      ⎘ /memory-edit
    </button>
  </div>
</template>
