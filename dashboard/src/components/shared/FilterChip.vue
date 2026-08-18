<script setup>
import { computed } from 'vue'
import { Button } from '@oxhive/ui'
const props = defineProps({ label: String, value: String, active: Boolean, layer: String })
defineEmits(['select'])

const chipStyle = computed(() => {
  if (!props.active) return 'background:transparent; border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'
  if (props.layer === 'personal') return 'background:var(--hm-personal-bg); border-color:var(--hm-personal); color:var(--hm-personal)'
  if (props.layer === 'workspace') return 'background:var(--hm-workspace-bg); border-color:var(--hm-workspace); color:var(--hm-workspace)'
  if (props.layer === 'org') return 'background:var(--hm-org-bg); border-color:var(--hm-org); color:var(--hm-org)'
  return 'background:var(--hm-bg-elevated); border-color:var(--hm-border-default); color:var(--hm-text-primary)'
})
</script>

<template>
  <Button @click="$emit('select', value)" size="sm" :style="chipStyle">
    {{ label }}
  </Button>
</template>
