<script setup>
import { ref, computed } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import TagChip from './TagChip.vue'

const props = defineProps({ modelValue: { type: Array, default: () => [] } })
const emit = defineEmits(['update:modelValue'])

const tagSettings = useTagSettingsStore()
const inputValue = ref('')
const showSuggestions = ref(false)

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

function commit(rawTag) {
  const tag = rawTag.trim().toLowerCase()
  if (!tag) return
  const isProjectTag = tag.startsWith('project:')
  let next = props.modelValue.filter(t => !(isProjectTag && t.toLowerCase().startsWith('project:')))
  if (!next.includes(tag)) next = [...next, tag]
  emit('update:modelValue', next)
  inputValue.value = ''
  showSuggestions.value = false
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
      <TagChip v-for="tag in modelValue" :key="tag" :tag="tag" removable @remove="removeTag(tag)" />
      <input class="hm-input" style="width:120px; height:22px; font-size:10px; padding:0 6px"
        v-model="inputValue"
        placeholder="add tag…"
        @focus="showSuggestions = true"
        @keydown.enter.prevent="commit(inputValue)"
        @keydown.esc="showSuggestions = false"
        @blur="handleBlur" />
    </div>
    <div v-if="showSuggestions && suggestions.length"
      class="absolute left-0 mt-1 rounded-md py-1"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10; min-width:140px">
      <button v-for="s in suggestions" :key="s"
        class="block w-full text-left px-3 py-1.5 font-mono"
        style="font-size:11px; color:var(--hm-text-secondary)"
        @mousedown.prevent="selectSuggestion(s)">{{ s }}</button>
    </div>
  </div>
</template>
