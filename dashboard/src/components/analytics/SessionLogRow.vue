<script setup>
import { ref, computed } from 'vue'

const props = defineProps({ log: { type: Object, required: true } })
const expanded = ref(false)

const pct = computed(() => {
  if (!props.log.max_tokens || props.log.max_tokens <= 0) return 0
  return Math.min(100, Math.round((props.log.used_tokens / props.log.max_tokens) * 100))
})
const barColor = computed(() => props.log.truncated ? 'var(--hm-warning)' : 'var(--hm-accent)')
const relativeTime = computed(() => {
  const diffSec = Math.floor(Date.now() / 1000) - props.log.created_at
  if (diffSec < 60) return 'just now'
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`
  return `${Math.floor(diffSec / 86400)}d ago`
})
</script>

<template>
  <div class="py-3" style="border-bottom:0.5px solid var(--hm-border-subtle)">
    <div class="flex items-center gap-3 cursor-pointer" @click="expanded = !expanded">
      <span style="font-size:12px; color:var(--hm-text-primary); width:140px">{{ log.project_name }}</span>
      <span style="font-size:11px; color:var(--hm-text-tertiary); width:70px">{{ relativeTime }}</span>
      <div class="flex-1" style="height:6px; background:var(--hm-bg-elevated); border-radius:3px; overflow:hidden">
        <div :style="{ width: pct + '%', height: '100%', background: barColor }"></div>
      </div>
      <span style="font-size:11px; color:var(--hm-text-secondary); width:110px; text-align:right">
        {{ log.used_tokens }} / {{ log.max_tokens }} tok
      </span>
      <span v-if="log.truncated" style="font-size:10px; color:var(--hm-warning)">truncated</span>
    </div>

    <div v-if="expanded" class="mt-3 pl-4" style="font-size:11px">
      <div v-for="l in log.loaded" :key="l.id" class="flex justify-between py-1" style="color:var(--hm-text-secondary)">
        <span>{{ l.title }}</span>
        <span style="color:var(--hm-text-tertiary)">{{ l.tokens }} tok</span>
      </div>
      <div v-for="(s, i) in log.skipped" :key="i" class="flex justify-between py-1" style="color:var(--hm-text-tertiary)">
        <span>{{ s.query }}</span>
        <span>skipped &middot; {{ s.reason }}</span>
      </div>
      <div v-if="!log.loaded.length && !log.skipped.length" style="color:var(--hm-text-tertiary)">
        No recalls configured for this run.
      </div>
    </div>
  </div>
</template>
