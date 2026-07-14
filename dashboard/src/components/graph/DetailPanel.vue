<script setup>
import { computed } from 'vue'
import { useGraphStore } from '../../stores/graph.js'
import { useMemoriesStore } from '../../stores/memories.js'
import { useUiStore } from '../../stores/ui.js'
import LayerBadge from '../shared/LayerBadge.vue'
import TagChip from '../shared/TagChip.vue'
import CopyButton from '../shared/CopyButton.vue'

const graph = useGraphStore()
const memories = useMemoriesStore()
const ui = useUiStore()

const node = computed(() => memories.all.find(m => m.id === graph.selectedNodeId))
</script>

<template>
  <Transition name="panel">
    <div v-show="graph.selectedNodeId" class="flex flex-col h-full shrink-0"
      style="width:268px; background:var(--hm-bg-surface); border-left:0.5px solid var(--hm-border-subtle)">

      <div class="flex items-center justify-between px-5 py-2.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle)">
        <span style="font-size:13px; font-weight:500; color:var(--hm-text-primary)">Memory</span>
        <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="graph.selectedNodeId = null">✕</button>
      </div>

      <div v-if="node" class="flex-1 overflow-y-auto px-5 py-4">
        <div class="flex items-start justify-between gap-2 mb-2">
          <span style="font-size:14px; font-weight:500; color:var(--hm-text-primary)">{{ node.title }}</span>
          <LayerBadge :layer="node.layer" />
        </div>
        <span class="font-mono block mb-4" style="font-size:10px; color:var(--hm-text-tertiary)">{{ node.id }}</span>

        <p style="font-size:12px; color:var(--hm-text-secondary); line-height:1.5" class="mb-4">
          {{ node.content?.slice(0, 200) }}{{ (node.content?.length || 0) > 200 ? '…' : '' }}
        </p>

        <div class="flex flex-wrap gap-1.5 mb-5">
          <TagChip v-for="tag in node.tags" :key="tag" :tag="tag" />
        </div>

        <div class="flex flex-col gap-2">
          <CopyButton :command="`/memory-edit ${node.id}`" label="/memory-edit" />
          <CopyButton :command="`/suggest-connections`" label="/suggest-connections" />
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.panel-enter-active, .panel-leave-active { transition: transform 0.2s, opacity 0.2s; }
.panel-enter-from, .panel-leave-to { transform: translateX(20px); opacity: 0; }
</style>
