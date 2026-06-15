<script setup>
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'

const props = defineProps({ conflict: Object })
const ui = useUiStore()
const fb = useFeedbackStore()
</script>

<template>
  <div class="rounded-lg p-4 mb-3"
    style="background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-default)">

    <div class="flex items-start justify-between mb-3">
      <div>
        <span class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)">{{ conflict.id }}</span>
        <p class="mt-1" style="font-size:13px; font-weight:500; color:var(--hm-text-primary)">{{ conflict.title || 'Conflict' }}</p>
      </div>
    </div>

    <!-- Two-column diff -->
    <div class="grid grid-cols-2 gap-2 mb-3">
      <div class="rounded p-2" style="background:var(--hm-danger-bg); border:0.5px solid var(--hm-danger-border)">
        <p class="font-mono mb-1" style="font-size:9px; color:var(--hm-danger)">INCOMING</p>
        <p style="font-size:11px; color:var(--hm-text-secondary)">{{ conflict.incoming }}</p>
      </div>
      <div class="rounded p-2" style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default)">
        <p class="font-mono mb-1" style="font-size:9px; color:var(--hm-text-tertiary)">CURRENT</p>
        <p style="font-size:11px; color:var(--hm-text-secondary)">{{ conflict.current }}</p>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <button class="hm-btn hm-btn-primary hm-btn-sm"
        @click="fb.resolveConflict(conflict.id,'keep')">Keep current</button>
      <button class="hm-btn hm-btn-default hm-btn-sm"
        @click="fb.resolveConflict(conflict.id,'restore')">Use incoming</button>
      <button class="hm-btn hm-btn-ghost hm-btn-sm ml-auto"
        @click="ui.copyToClipboard(`/memory-merge ${conflict.id}`)">⎘ merge command</button>
    </div>
  </div>
</template>
