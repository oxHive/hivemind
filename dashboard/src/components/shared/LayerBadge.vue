<script setup>
defineProps({ layer: String })
</script>

<template>
  <span
    class="inline-block rounded-sm px-2 py-0.5 text-[10px] font-mono"
    :style="layer === 'personal'
      ? 'background:#0d2820; color:#1d9e75'
      : 'background:#1a1830; color:#7f77dd'"
  >{{ layer }}</span>
</template>
