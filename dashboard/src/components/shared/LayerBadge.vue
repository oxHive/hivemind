<script setup>
import { computed } from 'vue'
const props = defineProps({ layer: String })
const resolved = computed(() => props.layer === 'personal' ? 'personal' : 'workspace')
</script>

<template>
  <span
    class="inline-block rounded-sm px-2 py-0.5 text-[10px] font-mono"
    :style="resolved === 'personal'
      ? 'background:var(--hm-personal-bg); color:var(--hm-personal)'
      : 'background:var(--hm-workspace-bg); color:var(--hm-workspace)'"
  >{{ resolved }}</span>
</template>
