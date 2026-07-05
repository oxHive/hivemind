/** Format a unix-seconds timestamp as e.g. "Jul 4". */
export function fmtDate(unixSeconds) {
  if (!unixSeconds) return ''
  return new Date(unixSeconds * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}
