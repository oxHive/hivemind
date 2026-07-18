<script setup>
import MemoryList from '../components/memories/MemoryList.vue'
import MemoryDetail from '../components/memories/MemoryDetail.vue'
import PendingBar from '../components/shared/PendingBar.vue'
</script>

<template>
  <div class="flex flex-col flex-1 overflow-hidden">
    <PendingBar />
    <div class="flex flex-1 overflow-hidden">
      <MemoryList />
      <MemoryDetail />
    </div>
  </div>
</template>
