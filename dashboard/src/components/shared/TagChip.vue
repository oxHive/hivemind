<script setup>
import { computed, ref, nextTick } from 'vue'
import { PhX } from '@phosphor-icons/vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'

const props = defineProps({ tag: String, removable: Boolean, editable: Boolean })
const emit = defineEmits(['remove', 'edit'])

const tagSettings = useTagSettingsStore()
const color = computed(() => tagSettings.colorFor(props.tag))

const editing = ref(false)
const editValue = ref('')
const editInput = ref(null)

async function startEdit() {
  if (!props.editable) return
  editValue.value = props.tag
  editing.value = true
  await nextTick()
  editInput.value?.focus()
  editInput.value?.select()
}

function commitEdit() {
  if (!editing.value) return
  editing.value = false
  const next = editValue.value.trim().toLowerCase()
  if (!next || next === props.tag) return
  emit('edit', props.tag, next)
}

function cancelEdit() {
  editing.value = false
}
</script>

<template>
  <span v-if="editing" class="inline-flex items-center rounded-sm px-1.5 py-0.5"
    style="background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-default)">
    <input ref="editInput" v-model="editValue"
      class="hm-input text-[10px] font-mono" style="width:110px; height:16px; padding:0 2px; border:none; background:transparent"
      @keydown.enter.prevent="commitEdit"
      @keydown.esc="cancelEdit"
      @blur="commitEdit" />
  </span>
  <span v-else class="inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-[10px] font-mono"
    :style="color
      ? `background:${color}22; color:${color}; border:0.5px solid ${color}55`
      : 'background:var(--hm-bg-elevated); color:var(--hm-text-tertiary); border:0.5px solid var(--hm-border-subtle)'"
    @dblclick="startEdit">
    {{ tag }}
    <button v-if="removable" @click.stop="$emit('remove')" aria-label="Remove tag"
      class="inline-flex items-center tag-remove-btn"
      :style="color ? `color:${color}` : 'color:var(--hm-text-tertiary)'">
      <PhX :size="10" weight="bold" />
    </button>
  </span>
</template>

<style scoped>
.tag-remove-btn:hover {
  color: var(--hm-text-primary) !important;
}
</style>
