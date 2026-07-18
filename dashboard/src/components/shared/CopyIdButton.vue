<script setup>
import { useUiStore } from '../../stores/ui.js'
const props = defineProps({ id: String })
const ui = useUiStore()
</script>

<template>
  <button
    class="hm-btn hm-btn-ghost hm-btn-sm"
    style="padding:0 6px; font-size:15px; line-height:1"
    title="Copy ID"
    @click.stop="ui.copyToClipboard(props.id)"
  >⎘</button>
</template>
