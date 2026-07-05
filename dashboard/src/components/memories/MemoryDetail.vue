<script setup>
import { ref } from 'vue'
import { useMemoriesStore } from '../../stores/memories.js'
import { useUiStore } from '../../stores/ui.js'
import { useGraphStore } from '../../stores/graph.js'
import LayerBadge from '../shared/LayerBadge.vue'
import TagChip from '../shared/TagChip.vue'
import DeleteConfirmModal from './DeleteConfirmModal.vue'
import { fmtDate } from '../../lib/format.js'
import { createFeedback } from '../../api/feedback.js'

const memories = useMemoriesStore()
const ui = useUiStore()
const graph = useGraphStore()

const showDeleteModal = ref(false)
const newTagInput = ref('')
const addingTag = ref(false)
const flagOpen = ref(false)

async function flag(signal) {
  flagOpen.value = false
  await createFeedback({ memory_id: memories.selected.id, signal })
  ui.showToast(`Flagged as ${signal}`)
}

function removeTag(tag) {
  memories.draft.tags = memories.draft.tags.filter(t => t !== tag)
}

function addTag() {
  const t = newTagInput.value.trim()
  if (t && !memories.draft.tags.includes(t)) memories.draft.tags.push(t)
  newTagInput.value = ''
  addingTag.value = false
}

async function handleSave() {
  await memories.save()
  ui.showToast('Changes saved')
}

async function handleDelete() {
  const id = memories.selected.id
  showDeleteModal.value = false
  await memories.remove(id)
  ui.showToast('Memory deleted')
}
</script>

<template>
  <div class="flex flex-col h-full flex-1" style="background:var(--hm-bg-base)">

    <!-- Empty state -->
    <div v-if="!memories.selected" class="flex flex-col items-center justify-center h-full gap-2"
      style="color:var(--hm-text-tertiary)">
      <span style="font-size:20px">←</span>
      <p style="font-size:13px">Select a memory to view or edit</p>
    </div>

    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center justify-between px-4 py-2"
        style="border-bottom:0.5px solid var(--hm-border-subtle)">
        <span class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)">
          {{ memories.selected.id }}
        </span>
        <div class="flex gap-1">
          <div class="relative">
            <button class="hm-btn hm-btn-ghost hm-btn-sm" title="Flag for review"
              @click="flagOpen = !flagOpen">⚑</button>
            <div v-if="flagOpen" class="absolute right-0 mt-1 rounded-md py-1"
              style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10">
              <button v-for="r in ['incorrect','outdated','duplicate','other']" :key="r"
                class="block w-full text-left px-3 py-1.5"
                style="font-size:12px; color:var(--hm-text-secondary); background:none; border:none; cursor:pointer"
                @click="flag(r)">{{ r }}</button>
            </div>
          </div>
          <button class="hm-btn hm-btn-danger hm-btn-sm" @click="showDeleteModal = true">Delete</button>
        </div>
      </div>

      <!-- Body -->
      <div class="flex-1 overflow-y-auto px-4 py-3">
        <!-- Title -->
        <label class="hm-label" for="mem-title">TITLE</label>
        <input
          id="mem-title"
          class="hm-input mb-4"
          :value="memories.draft?.title"
          @input="memories.draft.title = $event.target.value"
        />

        <!-- Content -->
        <label class="hm-label" for="mem-content">CONTENT</label>
        <textarea
          id="mem-content"
          class="hm-input mb-4 resize-none"
          style="height:40vh; min-height:160px; padding:8px 10px; font-family:var(--hm-font-mono); font-size:12px; line-height:1.6; background:var(--hm-mono-bg)"
          :value="memories.draft?.content"
          @input="memories.draft.content = $event.target.value"
        ></textarea>

        <!-- Tags -->
        <label class="hm-label" id="mem-tags-label">TAGS</label>
        <div class="flex flex-wrap gap-1.5 p-2 mb-4 rounded-md"
          aria-labelledby="mem-tags-label"
          style="border:0.5px solid var(--hm-border-subtle); min-height:36px">
          <TagChip
            v-for="tag in memories.draft?.tags" :key="tag"
            :tag="tag" :removable="true"
            @remove="removeTag(tag)" />
          <template v-if="addingTag">
            <input class="hm-input" style="width:100px; height:22px; font-size:10px; padding:0 6px"
              v-model="newTagInput" @keydown.enter="addTag" @keydown.esc="addingTag = false" @blur="addTag" />
          </template>
          <button v-else class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)"
            @click="addingTag = true">+ add tag</button>
        </div>

        <!-- Connections -->
        <template v-if="memories.selected && graph.edgesFor(memories.selected.id).length">
          <label class="hm-label">CONNECTIONS</label>
          <div class="flex flex-col gap-1">
            <div v-for="edge in graph.edgesFor(memories.selected.id)" :key="edge.id"
              class="flex items-center justify-between px-2 py-1.5 rounded-md cursor-pointer"
              style="border:0.5px solid var(--hm-border-subtle); font-size:12px"
              @click="memories.select(memories.all.find(m => m.id === (edge.source_id === memories.selected.id ? edge.target_id : edge.source_id)))">
              <span class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary); margin-right:8px">
                {{ edge.relationship }}
              </span>
              <span style="flex:1; color:var(--hm-text-primary)">
                {{ memories.all.find(m => m.id === (edge.source_id === memories.selected.id ? edge.target_id : edge.source_id))?.title || edge.target_id }}
              </span>
              <span style="color:var(--hm-text-tertiary)">→</span>
            </div>
          </div>
        </template>
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-between px-4 py-2"
        style="border-top:0.5px solid var(--hm-border-subtle)">
        <span class="flex items-center gap-2 font-mono" style="font-size:11px; color:var(--hm-text-tertiary)">
          updated {{ fmtDate(memories.selected.updated_at) }}
          <LayerBadge :layer="memories.selected.layer" />
        </span>
        <button class="hm-btn hm-btn-primary hm-btn-sm"
          :disabled="!memories.dirty || memories.saving"
          @click="handleSave">
          {{ memories.saving ? 'Saving…' : 'Save' }}
        </button>
      </div>
    </template>

    <DeleteConfirmModal
      v-if="showDeleteModal"
      :mem="memories.selected"
      @confirm="handleDelete"
      @cancel="showDeleteModal = false"
    />
  </div>
</template>
