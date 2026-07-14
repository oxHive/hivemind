<script setup>
import { useThemeStore } from '../../stores/theme.js'

const theme = useThemeStore()

const options = [
  { id: 'system', label: 'System default' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
]
</script>

<template>
  <div>
    <p class="hm-label mb-4">APPEARANCE</p>
    <div class="flex gap-1.5">
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
  </div>
</template>
