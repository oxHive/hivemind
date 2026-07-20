<script setup>
import { ref } from 'vue'
import DangerModal from './DangerModal.vue'
const showModal = ref(false)
</script>

<template>
  <div>
    <p class="hm-label mb-4" style="color:var(--hm-danger)">DANGER ZONE</p>
    <div class="rounded-lg p-5" style="background:var(--hm-danger-bg); border:0.5px solid var(--hm-danger-border)">
      <p style="font-size:13px; font-weight:500; color:var(--hm-text-primary)" class="mb-1.5">Clear all memories</p>
      <p style="font-size:12px; color:var(--hm-text-secondary)" class="mb-4">
        Permanently deletes all memories, edges, and feedback. This cannot be undone.
      </p>
      <button class="hm-btn hm-btn-danger" @click="showModal = true">Clear all ⚠</button>
    </div>
    <DangerModal v-if="showModal" @close="showModal = false" />
  </div>
</template>
