<script setup>
import { ref, computed } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import TagChip from './TagChip.vue'
import ConfirmModal from './ConfirmModal.vue'

const props = defineProps({ modelValue: { type: Array, default: () => [] } })
const emit = defineEmits(['update:modelValue'])

const tagSettings = useTagSettingsStore()
const inputValue = ref('')
const showSuggestions = ref(false)
const pendingReplace = ref(null)
const rejectionError = ref('')

// project:* is the single-value namespace that identifies what the memory
// belongs to — surface it first regardless of storage order, same as MemoryCard.
const displayTags = computed(() => {
  const tags = props.modelValue
  const projectTag = tags.find(t => t.toLowerCase().startsWith('project:'))
  if (!projectTag) return tags
  return [projectTag, ...tags.filter(t => t !== projectTag)]
})

const suggestions = computed(() => {
  const raw = inputValue.value.trim()
  if (!raw) return []
  const colonIdx = raw.indexOf(':')
  if (colonIdx === -1) {
    return Object.keys(tagSettings.namespaces)
      .filter(ns => ns.startsWith(raw.toLowerCase()))
      .map(ns => `${ns}:`)
  }
  const ns = raw.slice(0, colonIdx).toLowerCase()
  const partial = raw.slice(colonIdx + 1).toLowerCase()
  const entry = tagSettings.namespaces[ns]
  if (!entry) return []
  return entry.values
    .filter(v => v.startsWith(partial))
    .map(v => `${ns}:${v}`)
})

// Enforces "fixed" namespaces client-side too — the backend rejects the
// same case on save, but catching it here gives immediate feedback instead
// of a failed save later.
function rejectionReason(tag) {
  const idx = tag.indexOf(':')
  if (idx === -1) return null
  const ns = tag.slice(0, idx)
  const value = tag.slice(idx + 1)
  const entry = tagSettings.namespaces[ns]
  if (!entry || entry.values_mode !== 'fixed' || !entry.values.length) return null
  if (entry.values.some(v => v.toLowerCase() === value.toLowerCase())) return null
  return `${ns} only allows: ${entry.values.join(', ')}`
}

function commit(rawTag) {
  const tag = rawTag.trim().toLowerCase()
  if (!tag) return
  const reason = rejectionReason(tag)
  if (reason) {
    rejectionError.value = reason
    return
  }
  rejectionError.value = ''
  maybeConfirmAndApply(null, tag)
  inputValue.value = ''
  showSuggestions.value = false
}

function handleEdit(oldTag, rawNewTag) {
  const newTag = rawNewTag.trim().toLowerCase()
  if (!newTag || newTag === oldTag) return
  const reason = rejectionReason(newTag)
  if (reason) {
    rejectionError.value = reason
    return
  }
  rejectionError.value = ''
  maybeConfirmAndApply(oldTag, newTag)
}

// oldTag: the tag being replaced (edit) or null (new tag being added).
// Project tags are single-value, so swapping one — via add or edit — always
// needs confirmation; editing a project tag also needs confirmation even
// when the new value isn't itself a project tag, since it changes what
// project the memory belongs to.
function maybeConfirmAndApply(oldTag, newTag) {
  const oldWasProjectTag = oldTag?.toLowerCase().startsWith('project:')
  const conflictingProjectTag = newTag.startsWith('project:')
    ? props.modelValue.find(t => t !== oldTag && t.toLowerCase().startsWith('project:'))
    : null
  if (oldWasProjectTag || conflictingProjectTag) {
    pendingReplace.value = { oldTag: conflictingProjectTag ?? oldTag, newTag, removeTag: oldTag }
    return
  }
  applyTagChange(oldTag, newTag)
}

function applyTagChange(removeTag, newTag) {
  let next = removeTag ? props.modelValue.filter(t => t !== removeTag) : [...props.modelValue]
  if (newTag.startsWith('project:')) {
    next = next.filter(t => !t.toLowerCase().startsWith('project:'))
  }
  if (!next.includes(newTag)) next = [...next, newTag]
  emit('update:modelValue', next)
}

function confirmReplace() {
  applyTagChange(pendingReplace.value.removeTag, pendingReplace.value.newTag)
  pendingReplace.value = null
}

function cancelReplace() {
  pendingReplace.value = null
}

function selectSuggestion(s) {
  commit(s)
}

function removeTag(tag) {
  emit('update:modelValue', props.modelValue.filter(t => t !== tag))
}

function handleBlur() {
  setTimeout(() => { showSuggestions.value = false }, 150)
}
</script>

<template>
  <div class="relative">
    <div class="flex flex-wrap gap-1.5 p-2.5 rounded-md" style="border:0.5px solid var(--hm-border-subtle); min-height:40px">
      <TagChip v-for="tag in displayTags" :key="tag" :tag="tag" removable editable @remove="removeTag(tag)" @edit="handleEdit" />
      <input class="hm-input" style="width:120px; height:22px; font-size:10px; padding:0 6px"
        v-model="inputValue"
        placeholder="add tag…"
        @focus="showSuggestions = true"
        @input="rejectionError = ''"
        @keydown.enter.prevent="commit(inputValue)"
        @keydown.esc="showSuggestions = false"
        @blur="handleBlur" />
    </div>
    <p v-if="rejectionError" class="mt-1" style="font-size:10px; color:var(--hm-danger)">{{ rejectionError }}</p>
    <div v-if="showSuggestions && suggestions.length"
      class="absolute left-0 mt-1 rounded-md py-1"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10; min-width:140px">
      <button v-for="s in suggestions" :key="s"
        class="block w-full text-left px-3 py-1.5 font-mono"
        style="font-size:11px; color:var(--hm-text-secondary)"
        @mousedown.prevent="selectSuggestion(s)">{{ s }}</button>
    </div>
    <ConfirmModal v-if="pendingReplace"
      title="Replace project tag?"
      :body="`This memory is already tagged ${pendingReplace.oldTag}. Replace it with ${pendingReplace.newTag}?`"
      confirm-label="Replace"
      @confirm="confirmReplace"
      @cancel="cancelReplace" />
  </div>
</template>
