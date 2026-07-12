<script setup>
defineProps({ message: String, hint: { type: String, default: '' } })
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-2 h-full text-center px-6"
    style="color:var(--hm-text-tertiary)">
    <svg width="28" height="28" viewBox="0 0 16 16" aria-hidden="true" class="mb-1">
      <polygon points="8,1.5 13.6,4.75 13.6,11.25 8,14.5 2.4,11.25 2.4,4.75"
        fill="none" stroke="var(--hm-border-strong)" stroke-width="1" />
      <circle cx="8" cy="8" r="1.5" fill="var(--hm-border-strong)" />
    </svg>
    <p style="font-size:13px; color:var(--hm-text-secondary)">{{ message }}</p>
    <p v-if="hint" style="font-size:12px">{{ hint }}</p>
  </div>
</template>
