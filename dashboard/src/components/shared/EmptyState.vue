<script setup>
defineProps({ message: String, icon: { type: String, default: '○' } })
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-3 h-full"
    style="color:var(--hm-text-tertiary)">
    <span style="font-size:40px; opacity:0.25">{{ icon }}</span>
    <p style="font-size:13px">{{ message }}</p>
  </div>
</template>
