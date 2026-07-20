<script setup>
import { ref, onMounted } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import { useUiStore } from '../../stores/ui.js'
import TagChip from '../shared/TagChip.vue'

const tagSettings = useTagSettingsStore()
const ui = useUiStore()
const newNamespaceName = ref('')
const newValueInput = ref({})
const saving = ref(false)
const error = ref('')
const pendingDelete = ref(null)

const SWATCHES = ['#4a9eff', '#e0607e', '#5fb8b0', '#a875d1', '#1d9e75', '#7f77dd', '#ba7517', '#d9534f']
const HEX_RE = /^#[0-9a-fA-F]{3,8}$/

onMounted(() => {
  if (!tagSettings.loaded) tagSettings.fetchNamespaces()
})

function setColor(ns, color) {
  tagSettings.namespaces[ns].color = color
}

function addValue(ns) {
  const v = (newValueInput.value[ns] || '').trim().toLowerCase()
  if (v && !tagSettings.namespaces[ns].values.includes(v)) {
    tagSettings.namespaces[ns].values.push(v)
  }
  newValueInput.value[ns] = ''
}

function removeValue(ns, v) {
  tagSettings.namespaces[ns].values = tagSettings.namespaces[ns].values.filter(x => x !== v)
}

function addNamespace() {
  const name = newNamespaceName.value.trim().toLowerCase()
  if (!name || tagSettings.namespaces[name]) return
  tagSettings.namespaces[name] = {
    color: SWATCHES[0], values: [], single_value: false, description: '', values_mode: 'suggestion',
  }
  newNamespaceName.value = ''
}

function askDeleteNamespace(name) {
  if (pendingDelete.value === name) {
    delete tagSettings.namespaces[name]
    pendingDelete.value = null
  } else {
    pendingDelete.value = name
  }
}

async function save() {
  saving.value = true
  error.value = ''
  try {
    await tagSettings.save()
    ui.showToast('Tag namespaces saved')
  } catch {
    error.value = 'Save failed — check namespace colors and values, then try again.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div>
    <p class="hm-label mb-4">TAG NAMESPACES</p>
    <p v-if="!tagSettings.loaded" style="font-size:12px; color:var(--hm-text-tertiary)">Loading…</p>
    <template v-else>
      <p v-if="Object.keys(tagSettings.namespaces).length === 0" class="mb-4"
        style="font-size:12px; color:var(--hm-text-tertiary)">
        No namespaces yet. Tags like <code class="font-mono">topic:idea</code> get a color once you
        define <code class="font-mono">topic</code> below.
      </p>

      <div v-for="(ns, name) in tagSettings.namespaces" :key="name" class="rounded-md mb-3 p-3"
        style="background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-subtle)">
        <div class="flex items-center gap-2 mb-3">
          <TagChip :tag="`${name}:example`" />
          <span class="font-mono flex-1" style="font-size:11px; color:var(--hm-text-secondary)">{{ name }}</span>
          <button v-if="pendingDelete !== name" class="hm-btn hm-btn-ghost hm-btn-sm"
            style="color:var(--hm-text-tertiary)" @click="askDeleteNamespace(name)">Remove</button>
          <template v-else>
            <span style="font-size:11px; color:var(--hm-text-tertiary)">Remove {{ name }}:*?</span>
            <button class="hm-btn hm-btn-danger hm-btn-sm" @click="askDeleteNamespace(name)">Confirm</button>
            <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="pendingDelete = null">Cancel</button>
          </template>
        </div>

        <input class="hm-input mb-3" style="font-size:11px"
          v-model="ns.description" placeholder="What does this namespace mean? Shown to AI agents and in this UI." />

        <div class="flex items-center gap-1.5 mb-3">
          <button v-for="c in SWATCHES" :key="c"
            class="rounded-full"
            style="width:16px; height:16px; border:1px solid var(--hm-border-default); flex-shrink:0"
            :style="{ background: c, outline: ns.color === c ? '2px solid var(--hm-accent)' : 'none', outlineOffset: '1px' }"
            :aria-label="`Use color ${c}`"
            @click="setColor(name, c)"></button>
          <input class="hm-input font-mono" style="width:88px; height:22px; font-size:10px; padding:0 6px"
            v-model="ns.color"
            :style="!HEX_RE.test(ns.color) ? `border-color:var(--hm-danger-border)` : ''" />
        </div>

        <label class="flex items-center gap-2 mb-3 cursor-pointer">
          <input type="checkbox" v-model="ns.single_value" class="w-3.5 h-3.5" />
          <span style="font-size:11px; color:var(--hm-text-secondary)">
            Only one {{ name }}:* tag per memory
          </span>
        </label>

        <div class="flex flex-wrap items-center gap-1.5 mb-2">
          <TagChip v-for="v in ns.values" :key="v" :tag="`${name}:${v}`" removable @remove="removeValue(name, v)" />
          <input class="hm-input" style="width:100px; height:22px; font-size:10px; padding:0 6px"
            v-model="newValueInput[name]" placeholder="add value"
            @keydown.enter="addValue(name)" />
        </div>

        <div v-if="ns.values.length" class="flex items-center gap-4" style="font-size:11px; color:var(--hm-text-secondary)">
          <label class="flex items-center gap-1.5 cursor-pointer">
            <input type="radio" :name="`values-mode-${name}`" class="w-3 h-3"
              :checked="(ns.values_mode || 'suggestion') === 'suggestion'"
              @change="ns.values_mode = 'suggestion'" />
            Suggestions — free text still allowed
          </label>
          <label class="flex items-center gap-1.5 cursor-pointer">
            <input type="radio" :name="`values-mode-${name}`" class="w-3 h-3"
              :checked="ns.values_mode === 'fixed'"
              @change="ns.values_mode = 'fixed'" />
            Fixed — only listed values allowed
          </label>
        </div>
      </div>

      <div class="flex items-center gap-2 mb-4 rounded-md p-3"
        style="border:1px dashed var(--hm-border-default)">
        <input class="hm-input" style="width:160px" v-model="newNamespaceName" placeholder="new namespace"
          @keydown.enter="addNamespace" />
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="addNamespace">+ Add namespace</button>
      </div>

      <div class="flex items-center gap-3">
        <button class="hm-btn hm-btn-primary" :disabled="saving || !tagSettings.isDirty" @click="save">
          {{ saving ? 'Saving…' : 'Save tag namespaces' }}
        </button>
        <span v-if="error" style="font-size:11px; color:var(--hm-danger)">{{ error }}</span>
      </div>
    </template>
  </div>
</template>
