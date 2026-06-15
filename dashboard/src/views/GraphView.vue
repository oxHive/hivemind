<script setup>
import { useMemoriesStore } from '../stores/memories.js'
import GraphCanvas from '../components/graph/GraphCanvas.vue'
import GraphToolbar from '../components/graph/GraphToolbar.vue'
import PendingBar from '../components/graph/PendingBar.vue'
import DetailPanel from '../components/graph/DetailPanel.vue'
import RelationshipPicker from '../components/graph/RelationshipPicker.vue'
import Legend from '../components/graph/Legend.vue'
import EmptyState from '../components/shared/EmptyState.vue'

const memories = useMemoriesStore()
</script>

<template>
  <div class="flex flex-1 overflow-hidden">
    <!-- Left: canvas area -->
    <div class="flex flex-col flex-1 overflow-hidden">
      <GraphToolbar />
      <PendingBar />
      <div class="flex-1 relative overflow-hidden">
        <EmptyState
          v-if="!memories.all.length"
          message="No connections to display. Store some memories first."
          icon="◎"
        />
        <GraphCanvas v-else />
      </div>
      <Legend />
    </div>

    <!-- Right: detail panel (slides in) -->
    <DetailPanel />

    <!-- Floating picker when connecting -->
    <RelationshipPicker />
  </div>
</template>
