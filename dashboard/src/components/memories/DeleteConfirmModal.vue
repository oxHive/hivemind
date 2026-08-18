<script setup>
import { Modal } from '@oxhive/ui'
defineProps({ mem: Object })
defineEmits(['confirm', 'cancel'])
</script>

<template>
  <Modal
    title="Delete memory?"
    :body="`&quot;${mem?.title}&quot; will be permanently deleted. This cannot be undone.`"
    confirmLabel="Delete"
    :dangerous="true"
    @confirm="$emit('confirm')"
    @cancel="$emit('cancel')"
  />
</template>
