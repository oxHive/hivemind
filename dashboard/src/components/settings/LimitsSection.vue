<script setup>
import { ref, onMounted } from 'vue'
import { useContentLimitSettingsStore } from '../../stores/contentLimitSettings.js'
import { useUiStore } from '../../stores/ui.js'

const contentLimits = useContentLimitSettingsStore()
const ui = useUiStore()
const saving = ref(false)
const error = ref('')

onMounted(() => {
  if (!contentLimits.loaded) contentLimits.fetch()
})

async function save() {
  saving.value = true
  error.value = ''
  try {
    await contentLimits.save()
    ui.showToast('Content limits saved')
  } catch {
    error.value = 'Save failed — max_content_tokens must be a positive number.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div>
    <p class="hm-label mb-4">CONTENT LIMITS</p>
    <p v-if="!contentLimits.loaded" style="font-size:12px; color:var(--hm-text-tertiary)">Loading…</p>
    <template v-else>
      <div class="rounded-md mb-4 p-3" style="background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-subtle)">
        <p class="hm-label" style="margin-bottom:8px">MAX CONTENT TOKENS</p>
        <input class="hm-input font-mono mb-3" style="width:120px; font-size:12px"
          type="number" min="1" v-model.number="contentLimits.maxContentTokens" />
        <p style="font-size:11px; color:var(--hm-text-tertiary)">
          A single memory's title + content can't exceed this many tokens — enforced for
          <code class="font-mono">memory_store</code> and <code class="font-mono">memory_update</code>,
          for AI agents too. Content over the limit is rejected with an error explaining how to split it
          into an index memory plus child memories. Leaves headroom under the session-start recall budget
          so no single memory can dominate a recall on its own.
        </p>
      </div>

      <div class="flex items-center gap-3">
        <button class="hm-btn hm-btn-primary" :disabled="saving || !contentLimits.isDirty" @click="save">
          {{ saving ? 'Saving…' : 'Save content limits' }}
        </button>
        <span v-if="error" style="font-size:11px; color:var(--hm-danger)">{{ error }}</span>
      </div>
    </template>
  </div>
</template>
