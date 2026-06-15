<script setup>
defineProps({ tag: String, removable: Boolean })
defineEmits(['remove'])
</script>

<template>
  <span class="inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-[10px] font-mono"
    style="background:var(--hm-bg-elevated); color:var(--hm-text-tertiary); border:0.5px solid var(--hm-border-subtle)">
    {{ tag }}
    <button v-if="removable" @click.stop="$emit('remove')"
      class="leading-none hover:text-white" style="color:var(--hm-text-tertiary)">×</button>
  </span>
</template>
