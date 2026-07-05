<script setup>
defineProps({ message: String, hint: { type: String, default: '' } })
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-2 h-full text-center px-6"
    style="color:var(--hm-text-tertiary)">
    <p style="font-size:13px; color:var(--hm-text-secondary)">{{ message }}</p>
    <p v-if="hint" style="font-size:12px">{{ hint }}</p>
  </div>
</template>
