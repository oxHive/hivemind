<script setup>
import { useThemeStore } from '../../stores/theme.js'
import { useFontScaleStore, MIN_PERCENT, MAX_PERCENT } from '../../stores/fontScale.js'

const theme = useThemeStore()
const fontScale = useFontScaleStore()

const options = [
  { id: 'system', label: 'System default' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
]
</script>

<template>
  <div>
    <p class="hm-label mb-4">APPEARANCE</p>
    <div class="flex gap-1.5 mb-6">
      <button v-for="opt in options" :key="opt.id"
        class="hm-btn hm-btn-sm rounded-sm"
        :aria-pressed="theme.mode === opt.id"
        :style="theme.mode === opt.id
          ? 'background:var(--hm-bg-elevated); border-color:var(--hm-border-default); color:var(--hm-text-primary)'
          : 'background:transparent; border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'"
        @click="theme.setMode(opt.id)">
        {{ opt.label }}
      </button>
    </div>

    <p class="hm-label mb-4">FONT SIZE</p>
    <div class="flex items-center gap-3">
      <input type="range" :min="MIN_PERCENT" :max="MAX_PERCENT" step="5"
        :value="fontScale.percent" @input="fontScale.setPercent(+$event.target.value)"
        style="width:160px" />
      <span class="font-mono" style="font-size:12px; color:var(--hm-text-secondary); width:44px">
        {{ fontScale.percent }}%
      </span>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="fontScale.reset">Reset</button>
    </div>
    <p style="font-size:11px; color:var(--hm-text-tertiary)" class="mt-2">
      Scales the whole dashboard (text, spacing, layout) up or down. Saved on this device only.
    </p>
  </div>
</template>
