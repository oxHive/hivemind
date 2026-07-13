<script setup>
import { computed } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'

const props = defineProps({ tag: String, removable: Boolean })
defineEmits(['remove'])

const tagSettings = useTagSettingsStore()
const color = computed(() => tagSettings.colorFor(props.tag))
</script>

<template>
  <span class="inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-[10px] font-mono"
    :style="color
      ? `background:${color}22; color:${color}; border:0.5px solid ${color}55`
      : 'background:var(--hm-bg-elevated); color:var(--hm-text-tertiary); border:0.5px solid var(--hm-border-subtle)'">
    {{ tag }}
    <button v-if="removable" @click.stop="$emit('remove')"
      class="leading-none hover:text-white"
      :style="color ? `color:${color}` : 'color:var(--hm-text-tertiary)'">×</button>
  </span>
</template>
