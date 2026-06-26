<script setup>
defineProps({
  dot: String,      // 'green' | 'amber' | 'red' | 'gray'
  text: String,
  mono: { type: Boolean, default: true },
})
</script>

<template>
  <div class="flex items-center gap-2 py-0.5">
    <span class="dot rounded-full shrink-0" :class="`dot--${dot || 'gray'}`"></span>
    <span :class="mono ? 'font-mono' : ''" style="font-size:11px; color:var(--hm-text-secondary)">{{ text }}</span>
  </div>
</template>

<style scoped>
.dot { width: 6px; height: 6px; }
.dot--green  { background: var(--hm-personal); }
.dot--amber  { background: var(--hm-warning); }
.dot--red    { background: var(--hm-danger); }
.dot--gray   { background: var(--hm-text-tertiary); }
</style>
