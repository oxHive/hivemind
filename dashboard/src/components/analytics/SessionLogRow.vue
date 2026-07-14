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
  <div class="session-row" :class="{ 'session-row--expanded': expanded }">
    <div class="session-row__header" @click="expanded = !expanded">
      <svg class="session-row__chevron" width="8" height="8" viewBox="0 0 8 8" aria-hidden="true">
        <path d="M1 1l3 3-3 3" fill="none" stroke="var(--hm-text-tertiary)" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span style="font-size:12px; color:var(--hm-text-primary); width:140px">{{ log.project_name }}</span>
      <span style="font-size:11px; color:var(--hm-text-tertiary); width:70px">{{ relativeTime }}</span>
      <div class="flex-1" style="height:6px; background:var(--hm-bg-overlay); border-radius:3px; overflow:hidden">
        <div :style="{ width: pct + '%', height: '100%', background: barColor }"></div>
      </div>
      <span style="font-size:11px; color:var(--hm-text-secondary); width:110px; text-align:right; font-variant-numeric:tabular-nums">
        {{ log.used_tokens }} / {{ log.max_tokens }} tok
      </span>
      <span v-if="log.truncated" style="font-size:10px; color:var(--hm-warning); width:60px; text-align:right">truncated</span>
    </div>

    <div v-if="expanded" class="session-row__detail" style="font-size:11px">
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

<style scoped>
.session-row {
  border-bottom: 0.5px solid var(--hm-border-subtle);
  margin: 0 -12px;
}
.session-row:last-child { border-bottom: none; }
.session-row__header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  cursor: pointer;
  border-radius: 6px;
}
.session-row__header:hover { background: var(--hm-bg-overlay); }
.session-row__chevron {
  flex-shrink: 0;
  transition: transform 120ms ease;
}
.session-row--expanded .session-row__chevron { transform: rotate(90deg); }
.session-row__detail {
  padding: 2px 12px 12px 32px;
}
</style>
