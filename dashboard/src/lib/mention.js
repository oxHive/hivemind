// Mirrors the backend's `[phrase](kind:mem_xxx)` mention link syntax
// (src/store.rs RELATIONSHIP_LINK_RE) so an approved suggestion's content
// edit round-trips into the same auto-synced edge it was suggesting.

export function mentionMarkdown(edge) {
  const label = edge.link_text || edge.relationship
  return `See also: [${label}](${edge.relationship}:${edge.target_id})`
}

// Appends the mention line to `content` unless it's already present.
// Returns `content` unchanged when nothing needs to change.
export function withMention(content, edge) {
  const line = mentionMarkdown(edge)
  if (content && content.includes(line)) return content
  return content ? `${content}\n\n${line}` : line
}

// Preview of what approving `edge` will do to `content`: the trailing line
// of existing content for context, plus whatever withMention would append.
// Returns null when the edge is already reflected (nothing would change).
export function diffPreview(content, edge) {
  const before = content || ''
  const after = withMention(before, edge)
  if (after === before) return null
  const added = after.slice(before.length).replace(/^\n+/, '')
  const context = before.trim().split('\n').filter(Boolean).pop()
  return { context: context || '(empty memory)', added, removed: null }
}
