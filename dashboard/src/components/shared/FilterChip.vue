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
  <Button @click="$emit('select', value)" size="sm" class="filter-chip-btn" :style="chipStyle">
    {{ label }}
  </Button>
</template>

<style scoped>
/* This chip uses <Button> with no `variant` prop, defaulting to
   `oxui-btn-default`, which carries `.oxui-btn-default:hover { background:
   var(--hm-bg-elevated) }`. Pre-migration this was a bare `hm-btn hm-btn-sm`
   element with no matching :hover rule, so hovering it was a visual no-op.
   `chipStyle` always declares its own `background` (transparent, a layer
   color, or the active default), which normally outranks any class selector
   including :hover ones — so this rule is primarily defensive/documentary,
   guarding against the hover tint (which would clash with the layer color)
   reappearing if `chipStyle` is ever changed to omit `background`. */
.filter-chip-btn:hover {
  background: transparent;
}
</style>
