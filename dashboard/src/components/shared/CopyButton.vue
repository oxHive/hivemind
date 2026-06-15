<script setup>
import { useUiStore } from '../../stores/ui.js'
const props = defineProps({ command: String, label: String })
const ui = useUiStore()
</script>

<template>
  <button @click="ui.copyToClipboard(props.command)"
    class="hm-btn hm-btn-sm hm-btn-default inline-flex items-center gap-1.5 font-mono"
    style="border-color:var(--hm-border-default)">
    <span style="font-size:13px">⎘</span>
    <span>{{ label || command }}</span>
  </button>
</template>
