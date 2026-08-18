<script setup>
import { PhSparkle } from '@phosphor-icons/vue'
import { Button } from '@oxhive/ui'
import { useGraphStore } from '../../stores/graph.js'
import { useSuggestStore } from '../../stores/suggest.js'
const graph = useGraphStore()
const suggest = useSuggestStore()
</script>

<template>
  <div v-if="graph.pendingEdges.length && !suggest.panelOpen"
    class="flex items-center justify-between px-4 py-2 shrink-0"
    style="background:var(--hm-warning-bg); border-bottom:0.5px solid var(--hm-warning-border)">
    <span class="flex items-center gap-1.5" style="font-size:12px; color:var(--hm-warning)">
      <PhSparkle :size="13" weight="fill" />
      {{ graph.pendingEdges.length }} connection {{ graph.pendingEdges.length === 1 ? 'suggestion' : 'suggestions' }} pending review
    </span>
    <div class="flex items-center gap-4">
      <button class="pending-bar__link" @click="graph.rejectAllPending()">Reject all</button>
      <Button size="sm" class="accept-all-btn" style="background:var(--hm-accent); border-color:var(--hm-accent); color:var(--hm-bg-base); font-weight:600"
        @click="graph.acceptAllPending()">Accept all</Button>
    </div>
  </div>
</template>

<style scoped>
.pending-bar__link {
  background: none;
  border: none;
  padding: 0;
  font-size: var(--hm-text-sm);
  color: var(--hm-text-secondary);
  cursor: pointer;
}

.pending-bar__link:hover {
  color: var(--hm-text-primary);
  text-decoration: underline;
}

/* This CTA uses <Button> with no `variant` prop, defaulting to
   `oxui-btn-default`, which carries `.oxui-btn-default:hover { background:
   var(--hm-bg-elevated) }` — a dim neutral tint that would visibly wash out
   this solid accent-colored "Accept all" CTA on hover. Pre-migration this was
   a bare `hm-btn hm-btn-sm` element with no matching :hover rule, so hovering
   it was a visual no-op; the static inline `style` above always declares
   `background: var(--hm-accent)`, which normally outranks any class selector
   including :hover ones, so this rule is primarily defensive/documentary,
   pinning the intended hover background explicitly in case the inline style
   is ever refactored away from a literal always-on background. */
.accept-all-btn:hover {
  background: var(--hm-accent);
}
</style>
