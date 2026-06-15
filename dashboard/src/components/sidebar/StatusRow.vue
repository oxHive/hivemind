<script setup>
const props = defineProps({
  dot: String,      // 'green' | 'amber' | 'red' | 'gray'
  text: String,
  mono: { type: Boolean, default: true },
})

const dotColor = { green: '#1d9e75', amber: '#ba7517', red: '#a32d2d', gray: '#606060' }
</script>

<template>
  <div class="flex items-center gap-2 py-0.5">
    <span class="rounded-full shrink-0" :style="`width:6px; height:6px; background:${dotColor[dot] || dotColor.gray}`"></span>
    <span :class="mono ? 'font-mono' : ''" style="font-size:11px; color:var(--hm-text-secondary)">{{ text }}</span>
  </div>
</template>
