<script setup>
import { ref, computed, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { useMemoriesStore } from '../../stores/memories.js'
import { useUiStore } from '../../stores/ui.js'
import { useGraphStore } from '../../stores/graph.js'
import LayerBadge from '../shared/LayerBadge.vue'
import TagInput from '../shared/TagInput.vue'
import EmptyState from '../shared/EmptyState.vue'
import DeleteConfirmModal from './DeleteConfirmModal.vue'
import MarkdownContent from '../shared/MarkdownContent.vue'
import { fmtDate } from '../../lib/format.js'
import { createFeedback } from '../../api/feedback.js'
import { caretCoords } from '../../lib/caret.js'

const memories = useMemoriesStore()
const ui = useUiStore()
const graph = useGraphStore()

const showDeleteModal = ref(false)
const flagOpen = ref(false)
const contentView = ref('markdown') // 'markdown' | 'raw'

const contentEl = ref(null)
const mention = ref(null) // { start, query, top, left } | null
const mentionIndex = ref(0)

const mentionResults = computed(() => {
  if (!mention.value) return []
  const q = mention.value.query.toLowerCase()
  return memories.all
    .filter(m => m.id !== memories.selected?.id && m.title.toLowerCase().includes(q))
    .slice(0, 8)
})

function detectMention(ta) {
  const before = ta.value.slice(0, ta.selectionStart)
  const at = before.lastIndexOf('@')
  if (at === -1) { mention.value = null; return }
  const prev = at === 0 ? ' ' : before[at - 1]
  const query = before.slice(at + 1)
  if (!/\s/.test(prev) || /\s/.test(query) || query.length > 40) {
    mention.value = null
    return
  }
  const { top, left } = caretCoords(ta, at)
  mention.value = { start: at, query, top: top + 22, left }
  mentionIndex.value = 0
}

function onContentInput(e) {
  memories.draft.content = e.target.value
  detectMention(e.target)
}

function onContentKeydown(e) {
  if (!mention.value || !mentionResults.value.length) return
  const n = mentionResults.value.length
  if (e.key === 'ArrowDown') { e.preventDefault(); mentionIndex.value = (mentionIndex.value + 1) % n }
  else if (e.key === 'ArrowUp') { e.preventDefault(); mentionIndex.value = (mentionIndex.value - 1 + n) % n }
  else if (e.key === 'Enter') { e.preventDefault(); pickMention(mentionResults.value[mentionIndex.value]) }
  else if (e.key === 'Escape') { mention.value = null }
}

function pickMention(m) {
  const ta = contentEl.value
  const text = memories.draft.content
  const caret = ta.selectionStart
  // Strip brackets/parens from the link text: the backend's MENTION_RE is a plain
  // regex (no backslash-escape awareness), so an unescaped `]`/`)` in the title
  // would prematurely terminate the `[phrase](mem_id)` markdown link.
  const safeTitle = m.title.replace(/[[\]()]/g, '')
  const insert = `[${safeTitle}](${m.id})`
  const pos = mention.value.start + insert.length
  memories.draft.content = text.slice(0, mention.value.start) + insert + text.slice(caret)
  mention.value = null
  nextTick(() => { ta.focus(); ta.setSelectionRange(pos, pos) })
}

function goToMemory(id) {
  const target = memories.all.find(m => m.id === id)
  if (target) memories.select(target)
}

async function flag(signal) {
  flagOpen.value = false
  await createFeedback({ memory_id: memories.selected.id, signal })
  ui.showToast(`Flagged as ${signal}`)
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

function handleKeydown(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault()
    if (memories.selected && memories.dirty && !memories.saving) handleSave()
  }
}

onMounted(() => window.addEventListener('keydown', handleKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', handleKeydown))
</script>

<template>
  <div class="flex flex-col h-full flex-1" style="background:var(--hm-bg-base)">

    <!-- Empty state -->
    <EmptyState v-if="!memories.selected"
      message="Select a memory to view or edit"
      hint="Press / to search, Ctrl+S to save changes" />

    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center justify-between px-5 py-2.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle)">
        <span class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)">
          {{ memories.selected.id }}
        </span>
        <div class="flex gap-1">
          <div class="relative">
            <button class="hm-btn hm-btn-ghost hm-btn-sm" title="Flag for review"
              @click="flagOpen = !flagOpen" @keydown.esc="flagOpen = false">⚑</button>
            <div v-if="flagOpen" class="fixed inset-0" style="z-index:9" @click="flagOpen = false"></div>
            <div v-if="flagOpen" class="absolute right-0 mt-1 rounded-md py-1"
              style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10">
              <button v-for="r in ['incorrect','outdated','duplicate','other']" :key="r"
                class="flag-option block w-full text-left px-3 py-1.5"
                @click="flag(r)">{{ r }}</button>
            </div>
          </div>
          <button class="hm-btn hm-btn-danger hm-btn-sm" @click="showDeleteModal = true">Delete</button>
        </div>
      </div>

      <!-- Body -->
      <div class="flex-1 overflow-y-auto px-6 py-5">
        <!-- Title -->
        <label class="hm-label" for="mem-title">TITLE</label>
        <input
          id="mem-title"
          class="hm-input mb-6"
          :value="memories.draft?.title"
          @input="memories.draft.title = $event.target.value"
        />

        <!-- Content -->
        <div class="flex items-center justify-between mb-1.5">
          <label class="hm-label" style="margin-bottom:0" for="mem-content">CONTENT</label>
          <div class="content-toggle" role="tablist" aria-label="Content view">
            <button type="button" role="tab" :aria-selected="contentView === 'markdown'"
              class="content-toggle__btn" :class="{ 'content-toggle__btn--active': contentView === 'markdown' }"
              @click="contentView = 'markdown'">Markdown</button>
            <button type="button" role="tab" :aria-selected="contentView === 'raw'"
              class="content-toggle__btn" :class="{ 'content-toggle__btn--active': contentView === 'raw' }"
              @click="contentView = 'raw'">Raw</button>
          </div>
        </div>
        <div v-if="contentView === 'raw'" class="relative mb-6">
          <textarea
            id="mem-content"
            ref="contentEl"
            class="hm-input resize-none w-full"
            style="height:40vh; min-height:160px; padding:10px 12px; font-family:var(--hm-font-mono); font-size:12px; line-height:1.6; background:var(--hm-mono-bg)"
            :value="memories.draft?.content"
            @input="onContentInput"
            @keydown="onContentKeydown"
            @blur="mention = null"
          ></textarea>
          <div v-if="mention && mentionResults.length" class="mention-menu"
            :style="{ top: mention.top + 'px', left: mention.left + 'px' }">
            <button v-for="(m, i) in mentionResults" :key="m.id" type="button"
              class="mention-menu__item" :class="{ 'mention-menu__item--active': i === mentionIndex }"
              @mousedown.prevent="pickMention(m)">
              {{ m.title }}
            </button>
          </div>
        </div>
        <div v-else id="mem-content" class="mb-6 overflow-y-auto"
          style="height:40vh; min-height:160px; padding:10px 12px; border-radius:6px; border:0.5px solid var(--hm-border-default); background:var(--hm-mono-bg)">
          <MarkdownContent :text="memories.draft?.content" @navigate="goToMemory" />
        </div>

        <!-- Tags -->
        <label class="hm-label" id="mem-tags-label">TAGS</label>
        <div class="mb-6" aria-labelledby="mem-tags-label">
          <TagInput
            :model-value="memories.draft?.tags ?? []"
            @update:model-value="memories.draft.tags = $event" />
        </div>

        <!-- Connections -->
        <template v-if="memories.selected && graph.edgesFor(memories.selected.id).length">
          <label class="hm-label">CONNECTIONS</label>
          <div class="flex flex-col gap-1.5">
            <div v-for="edge in graph.edgesFor(memories.selected.id)" :key="edge.id"
              class="flex items-center justify-between px-3 py-2 rounded-md cursor-pointer"
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
      <div class="flex items-center justify-between px-5 py-3"
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

<style scoped>
.flag-option {
  font-size: 12px;
  color: var(--hm-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
}

.flag-option:hover,
.flag-option:focus-visible {
  background: var(--hm-bg-elevated);
  color: var(--hm-text-primary);
  outline: none;
}

.content-toggle {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: 6px;
  background: var(--hm-bg-elevated);
}

.content-toggle__btn {
  font-size: 10px;
  font-weight: 500;
  padding: 3px 8px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--hm-text-tertiary);
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.content-toggle__btn:hover {
  color: var(--hm-text-primary);
}

.content-toggle__btn:focus-visible {
  outline: 2px solid var(--hm-accent);
  outline-offset: -2px;
}

.content-toggle__btn--active {
  background: var(--hm-bg-overlay);
  color: var(--hm-text-primary);
}

.mention-menu {
  position: absolute;
  z-index: 20;
  min-width: 180px;
  max-width: 320px;
  padding: 4px 0;
  border-radius: 6px;
  background: var(--hm-bg-overlay);
  border: 0.5px solid var(--hm-border-default);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
}
.mention-menu__item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 5px 10px;
  font-size: 12px;
  color: var(--hm-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.mention-menu__item--active,
.mention-menu__item:hover {
  background: var(--hm-bg-elevated);
  color: var(--hm-text-primary);
}
</style>
