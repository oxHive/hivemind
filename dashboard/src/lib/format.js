/** Format a unix-seconds timestamp as e.g. "Jul 4". */
export function fmtDate(unixSeconds) {
  if (!unixSeconds) return ''
  return new Date(unixSeconds * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}

/** Turn a title into a filesystem-safe slug, e.g. for a download filename. */
export function slugify(text) {
  return (text || '')
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}
