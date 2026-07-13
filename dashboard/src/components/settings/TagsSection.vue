<script setup>
import { ref, onMounted } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import { useUiStore } from '../../stores/ui.js'
import TagChip from '../shared/TagChip.vue'

const tagSettings = useTagSettingsStore()
const ui = useUiStore()
const newNamespaceName = ref('')
const newValueInput = ref({})

const SWATCHES = ['#4a9eff', '#e0607e', '#5fb8b0', '#a875d1', '#1d9e75', '#7f77dd', '#ba7517', '#d9534f']

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
  if (name && !tagSettings.namespaces[name]) {
    tagSettings.namespaces[name] = { color: SWATCHES[0], values: [] }
  }
  newNamespaceName.value = ''
}

async function save() {
  await tagSettings.save()
  ui.showToast('Tag namespaces saved')
}
</script>

<template>
  <div>
    <p class="hm-label mb-4">TAG NAMESPACES</p>
    <p v-if="!tagSettings.loaded" style="font-size:12px; color:var(--hm-text-tertiary)">Loading…</p>
    <template v-else>
      <div v-for="(ns, name) in tagSettings.namespaces" :key="name" class="mb-6">
        <div class="flex items-center gap-2 mb-2">
          <TagChip :tag="`${name}:example`" />
          <span class="font-mono" style="font-size:11px; color:var(--hm-text-secondary)">{{ name }}</span>
        </div>
        <div class="flex items-center gap-1.5 mb-2">
          <button v-for="c in SWATCHES" :key="c"
            class="rounded-full"
            style="width:16px; height:16px; border:1px solid var(--hm-border-subtle)"
            :style="{ background: c }"
            @click="setColor(name, c)"></button>
          <input class="hm-input" style="width:80px; height:20px; font-size:10px" v-model="ns.color" />
        </div>
        <div class="flex flex-wrap gap-1.5 mb-2">
          <TagChip v-for="v in ns.values" :key="v" :tag="`${name}:${v}`" removable @remove="removeValue(name, v)" />
          <input class="hm-input" style="width:100px; height:22px; font-size:10px"
            v-model="newValueInput[name]" placeholder="add value"
            @keydown.enter="addValue(name)" />
        </div>
      </div>
      <div class="flex items-center gap-2 mb-4">
        <input class="hm-input" style="width:140px" v-model="newNamespaceName" placeholder="new namespace" />
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="addNamespace">+ Add namespace</button>
      </div>
      <button class="hm-btn hm-btn-primary" @click="save">Save tag namespaces</button>
    </template>
  </div>
</template>
