<script setup>
import { Toast } from '@oxhive/ui'
import { useUiStore } from '../../stores/ui.js'
const ui = useUiStore()
</script>

<template>
  <Toast :visible="ui.toast.visible" :message="ui.toast.message" />
</template>
