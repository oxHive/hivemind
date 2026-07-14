const PROPS = [
  'boxSizing', 'width',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
  'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
  'fontFamily', 'fontSize', 'fontWeight', 'fontStyle',
  'letterSpacing', 'lineHeight', 'textTransform', 'wordSpacing', 'textIndent',
]

// Position of the caret at `index` inside `textarea`, relative to its border
// box (mirror-div technique: replicate layout-affecting styles, measure a
// marker span placed after the text prefix).
export function caretCoords(textarea, index) {
  const div = document.createElement('div')
  const style = getComputedStyle(textarea)
  for (const p of PROPS) div.style[p] = style[p]
  div.style.position = 'absolute'
  div.style.visibility = 'hidden'
  div.style.whiteSpace = 'pre-wrap'
  div.style.wordWrap = 'break-word'
  div.textContent = textarea.value.slice(0, index)
  const marker = document.createElement('span')
  marker.textContent = '​'
  div.appendChild(marker)
  document.body.appendChild(div)
  const top = marker.offsetTop - textarea.scrollTop
  const left = marker.offsetLeft - textarea.scrollLeft
  div.remove()
  return { top, left }
}
