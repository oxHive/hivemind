<script setup>
import { ref } from 'vue'
import { useMemoriesStore } from '../../stores/memories.js'
import { useUiStore } from '../../stores/ui.js'

const emit = defineEmits(['close'])
const memories = useMemoriesStore()
const ui = useUiStore()

const title = ref('')
const content = ref('')
const tagsInput = ref('')
const layer = ref('workspace')
const saving = ref(false)

async function submit() {
  if (!title.value.trim() || !content.value.trim()) return
  saving.value = true
  try {
    await memories.create({
      title: title.value.trim(),
      content: content.value,
      tags: tagsInput.value.split(',').map(t => t.trim()).filter(Boolean),
      layer: layer.value,
    })
    ui.showToast('Memory created')
    emit('close')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="fixed inset-0 flex items-center justify-center" style="background:#000a; z-index:50"
    @click.self="emit('close')" @keydown.esc="emit('close')">
    <div class="rounded-lg p-4" role="dialog" aria-label="New memory"
      style="width:440px; background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-default)">
      <p class="mb-3" style="font-size:13px; font-weight:500; color:var(--hm-text-primary)">New memory</p>
      <label class="hm-label" for="nm-title">TITLE</label>
      <input id="nm-title" class="hm-input mb-3" v-model="title" autofocus />
      <label class="hm-label" for="nm-content">CONTENT</label>
      <textarea id="nm-content" class="hm-input mb-3 resize-none"
        style="height:120px; padding:8px 10px; font-family:var(--hm-font-mono); font-size:12px"
        v-model="content"></textarea>
      <label class="hm-label" for="nm-tags">TAGS (comma separated)</label>
      <input id="nm-tags" class="hm-input mb-3" v-model="tagsInput" placeholder="golang, preferences" />
      <label class="hm-label">LAYER</label>
      <div class="flex gap-1 mb-4">
        <button class="hm-btn hm-btn-sm"
          :style="layer==='workspace' ? 'background:var(--hm-workspace-bg); border-color:var(--hm-workspace); color:var(--hm-workspace)' : 'border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'"
          @click="layer='workspace'">workspace</button>
        <button class="hm-btn hm-btn-sm"
          :style="layer==='personal' ? 'background:var(--hm-personal-bg); border-color:var(--hm-personal); color:var(--hm-personal)' : 'border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'"
          @click="layer='personal'">personal</button>
      </div>
      <div class="flex justify-end gap-2">
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="emit('close')">Cancel</button>
        <button class="hm-btn hm-btn-primary hm-btn-sm"
          :disabled="saving || !title.trim() || !content.trim()" @click="submit">
          {{ saving ? 'Creating…' : 'Create' }}
        </button>
      </div>
    </div>
  </div>
</template>
