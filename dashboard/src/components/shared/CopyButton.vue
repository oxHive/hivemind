<script setup>
import { Button } from '@oxhive/ui'
import { useUiStore } from '../../stores/ui.js'
const props = defineProps({ command: String, label: String })
const ui = useUiStore()
</script>

<template>
  <Button @click="ui.copyToClipboard(props.command)"
    variant="default" size="sm" class="inline-flex items-center gap-1.5 font-mono"
    style="border-color:var(--hm-border-default)">
    <span style="font-size:13px">⎘</span>
    <span>{{ label || command }}</span>
  </Button>
</template>
