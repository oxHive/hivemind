<script setup>
import { computed } from 'vue'

const props = defineProps({
  visible: { type: Boolean, default: false },
  text: { type: String, default: '' },
  // Viewport-space anchor point — the tooltip centers above it, flipping
  // below when there isn't enough room near the top of the screen.
  x: { type: Number, default: 0 },
  y: { type: Number, default: 0 },
})

const GAP = 8

const style = computed(() => {
  const flipBelow = props.y < 48
  return {
    left: `${props.x}px`,
    top: flipBelow ? `${props.y + GAP}px` : `${props.y - GAP}px`,
    transform: `translate(-50%, ${flipBelow ? '0' : '-100%'})`,
  }
})
</script>

<template>
  <Teleport to="body">
    <div v-if="visible && text" class="hm-tooltip" :style="style">
      {{ text }}
    </div>
  </Teleport>
</template>

<style scoped>
.hm-tooltip {
  position: fixed;
  z-index: 1000;
  max-width: 320px;
  padding: 6px 9px;
  border-radius: 6px;
  border: 0.5px solid var(--hm-border-default);
  background: var(--hm-bg-overlay);
  color: var(--hm-text-primary);
  font-size: 11px;
  line-height: 1.4;
  white-space: normal;
  word-break: break-word;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  pointer-events: none;
  animation: hm-tooltip-in 0.1s ease-out;
}

@media (prefers-reduced-motion: reduce) {
  .hm-tooltip {
    animation: none;
  }
}

@keyframes hm-tooltip-in {
  from { opacity: 0; }
  to { opacity: 1; }
}
</style>
