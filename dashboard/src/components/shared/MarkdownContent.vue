<script setup>
import { computed } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const props = defineProps({ text: { type: String, default: '' } })

marked.setOptions({ breaks: true })

const html = computed(() => DOMPurify.sanitize(marked.parse(props.text || '')))
</script>

<template>
  <div class="hm-markdown" v-html="html"></div>
</template>

<style scoped>
.hm-markdown {
  font-size: 12px;
  line-height: 1.6;
  color: var(--hm-text-primary);
}
.hm-markdown :deep(p) { margin: 0 0 10px; }
.hm-markdown :deep(p:last-child) { margin-bottom: 0; }
.hm-markdown :deep(h1),
.hm-markdown :deep(h2),
.hm-markdown :deep(h3) {
  font-weight: 600;
  margin: 14px 0 6px;
  color: var(--hm-text-primary);
}
.hm-markdown :deep(h1:first-child),
.hm-markdown :deep(h2:first-child),
.hm-markdown :deep(h3:first-child) { margin-top: 0; }
.hm-markdown :deep(ul) { list-style: disc; margin: 0 0 10px; padding-left: 20px; }
.hm-markdown :deep(ol) { list-style: decimal; margin: 0 0 10px; padding-left: 20px; }
.hm-markdown :deep(li) { display: list-item; margin: 2px 0; }
.hm-markdown :deep(li)::marker { color: var(--hm-text-tertiary); }
.hm-markdown :deep(code) {
  font-family: var(--hm-font-mono);
  font-size: 11px;
  background: var(--hm-mono-bg);
  border: 0.5px solid var(--hm-mono-border);
  border-radius: 3px;
  padding: 1px 4px;
}
.hm-markdown :deep(pre) {
  font-family: var(--hm-font-mono);
  font-size: 11px;
  background: var(--hm-mono-bg);
  border: 0.5px solid var(--hm-mono-border);
  border-radius: 6px;
  padding: 10px 12px;
  overflow-x: auto;
  margin: 0 0 10px;
}
.hm-markdown :deep(pre code) {
  background: none;
  border: none;
  padding: 0;
}
.hm-markdown :deep(a) {
  color: var(--hm-accent);
}
.hm-markdown :deep(blockquote) {
  margin: 0 0 10px;
  padding-left: 10px;
  border-left: 2px solid var(--hm-border-default);
  color: var(--hm-text-secondary);
}
</style>
