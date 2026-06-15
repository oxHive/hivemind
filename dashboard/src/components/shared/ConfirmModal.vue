<script setup>
defineProps({ title: String, body: String, confirmLabel: String, dangerous: Boolean })
defineEmits(['confirm', 'cancel'])
</script>

<template>
  <div class="fixed inset-0 z-40 flex items-center justify-center"
    style="background:rgba(0,0,0,0.6)">
    <div class="rounded-lg p-6 w-96"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default)">
      <h3 class="text-base font-medium mb-3" style="color:var(--hm-text-primary)">{{ title }}</h3>
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
