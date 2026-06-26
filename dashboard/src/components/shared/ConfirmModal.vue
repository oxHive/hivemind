<script setup>
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue'

const props = defineProps({ title: String, body: String, confirmLabel: String, dangerous: Boolean })
const emit = defineEmits(['confirm', 'cancel'])

const modalRef = ref(null)

function trapFocus(e) {
  if (!modalRef.value) return
  const focusable = modalRef.value.querySelectorAll('button, [href], input, [tabindex]:not([tabindex="-1"])')
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.key === 'Tab') {
    if (e.shiftKey) {
      if (document.activeElement === first) { e.preventDefault(); last.focus() }
    } else {
      if (document.activeElement === last) { e.preventDefault(); first.focus() }
    }
  }
  if (e.key === 'Escape') emit('cancel')
}

onMounted(async () => {
  document.addEventListener('keydown', trapFocus)
  await nextTick()
  modalRef.value?.querySelector('button')?.focus()
})

onBeforeUnmount(() => {
  document.removeEventListener('keydown', trapFocus)
})
</script>

<template>
  <div class="fixed inset-0 z-40 flex items-center justify-center"
    style="background:rgba(0,0,0,0.6)"
    @click.self="$emit('cancel')">
    <div ref="modalRef"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-modal-title"
      class="rounded-lg p-6 w-96"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default)">
      <h3 id="confirm-modal-title" class="text-base font-medium mb-3" style="color:var(--hm-text-primary)">{{ title }}</h3>
      <p class="text-sm mb-5" style="color:var(--hm-text-secondary)">{{ body }}</p>
      <div class="flex justify-end gap-2">
        <button class="hm-btn hm-btn-default" @click="$emit('cancel')">Cancel</button>
        <button
          class="hm-btn"
          :class="dangerous ? 'hm-btn-danger' : 'hm-btn-primary'"
          @click="$emit('confirm')">
          {{ confirmLabel }}
        </button>
      </div>
    </div>
  </div>
</template>
