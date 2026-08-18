<script setup>
import { computed } from 'vue'
import { Badge } from '@oxhive/ui'

const props = defineProps({ layer: String })
const resolved = computed(() => {
  if (props.layer === 'personal') return 'personal'
  if (props.layer === 'org') return 'org'
  return 'workspace'
})
// Pre-mixed, hand-picked background tokens (not derived via color-mix) —
// kept as an exact override rather than passed through Badge's generic
// `color` prop. Badge's own `color-mix(in srgb, color 18%, transparent)`
// tint is composited over whatever sits behind the badge, so its resulting
// color varies by container and, per the arithmetic checked for this
// refactor, differs from these tokens by a visible margin in the light
// theme (~15-30/255 per channel depending on backdrop). Passing no `color`
// to Badge makes it fall back to its default style, and the `:style` bound
// here on the <Badge> usage wins over that default for the conflicting
// `background`/`color` properties (Vue merges a component's fallthrough
// attrs onto its root node after the component's own bindings), so the
// rendered badge is pixel-identical to the pre-refactor markup. Badge is
// used here purely for the shared span markup/sizing classes.
const styleFor = {
  personal: 'background:var(--hm-personal-bg); color:var(--hm-personal)',
  org: 'background:var(--hm-org-bg); color:var(--hm-org)',
  workspace: 'background:var(--hm-workspace-bg); color:var(--hm-workspace)',
}
</script>

<template>
  <Badge :label="resolved" :style="styleFor[resolved]" />
</template>
