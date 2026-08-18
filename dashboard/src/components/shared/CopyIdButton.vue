<script setup>
import { Button } from '@oxhive/ui'
import { useUiStore } from '../../stores/ui.js'
const props = defineProps({ id: String })
const ui = useUiStore()
</script>

<template>
  <Button
    variant="ghost" size="sm"
    style="padding:0 6px; font-size:15px; line-height:1"
    title="Copy ID"
    @click.stop="ui.copyToClipboard(props.id)"
  >⎘</Button>
</template>
