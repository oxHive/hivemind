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
