<script setup>
import ConfirmModal from '../shared/ConfirmModal.vue'
defineProps({ mem: Object })
defineEmits(['confirm', 'cancel'])
</script>

<template>
  <ConfirmModal
    title="Delete memory?"
    :body="`&quot;${mem?.title}&quot; will be permanently deleted. This cannot be undone.`"
    confirmLabel="Delete"
    :dangerous="true"
    @confirm="$emit('confirm')"
    @cancel="$emit('cancel')"
  />
</template>
