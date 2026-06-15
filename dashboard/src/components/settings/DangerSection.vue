<script setup>
import { ref } from 'vue'
import DangerModal from './DangerModal.vue'
const showModal = ref(false)
</script>

<template>
  <div>
    <p class="hm-label mb-3" style="color:var(--hm-danger)">DANGER ZONE</p>
    <div class="rounded-lg p-4" style="background:var(--hm-danger-bg); border:0.5px solid var(--hm-danger-border)">
      <p style="font-size:12px; font-weight:500; color:var(--hm-text-primary)" class="mb-1">Clear all memories</p>
      <p style="font-size:11px; color:var(--hm-text-secondary)" class="mb-3">
        Permanently deletes all memories, edges, and feedback. This cannot be undone.
      </p>
      <button class="hm-btn hm-btn-danger" @click="showModal = true">Clear all ⚠</button>
    </div>
    <DangerModal v-if="showModal" @close="showModal = false" />
  </div>
</template>
