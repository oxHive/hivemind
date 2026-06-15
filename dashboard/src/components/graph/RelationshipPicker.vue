<script setup>
import { ref } from 'vue'
import { useGraphStore } from '../../stores/graph.js'
import { useUiStore } from '../../stores/ui.js'

const graph = useGraphStore()
const ui = useUiStore()

const relationship = ref('related_to')
const custom = ref('')

const presets = ['related_to', 'depends_on', 'contradicts', 'extends', 'referenced_by']

async function confirm() {
  const rel = custom.value.trim() || relationship.value
  await graph.storeEdge(graph.pendingConnect.sourceId, graph.pendingConnect.targetId, rel)
  graph.cancelConnect()
  ui.showToast('Connection added')
}
</script>

<template>
  <div v-if="graph.pendingConnect"
    class="fixed inset-0 z-40 flex items-center justify-center"
    style="background:rgba(0,0,0,0.6)">
    <div class="rounded-lg p-5 w-80"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default)">
      <h3 class="mb-3 font-medium" style="font-size:13px; color:var(--hm-text-primary)">Add connection</h3>

      <label class="hm-label">RELATIONSHIP</label>
      <select v-model="relationship" class="hm-input mb-2">
        <option v-for="p in presets" :key="p" :value="p">{{ p }}</option>
      </select>

      <label class="hm-label">CUSTOM (overrides above)</label>
      <input v-model="custom" class="hm-input mb-4" placeholder="e.g. supports" />

      <div class="flex justify-end gap-2">
        <button class="hm-btn hm-btn-default" @click="graph.cancelConnect()">Cancel</button>
        <button class="hm-btn hm-btn-primary" @click="confirm()">Add connection</button>
      </div>
    </div>
  </div>
</template>
