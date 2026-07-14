<script setup>
import { ref, onMounted, watch } from 'vue'
import * as d3 from 'd3'

const props = defineProps({
  data: { type: Array, required: true },
  labelKey: { type: String, required: true },
  valueKey: { type: String, required: true },
  color: { type: [String, Function], default: 'var(--hm-accent)' },
  // 0 = auto-size to row count, so a 1-bar chart isn't padded to the same
  // height as a 10-bar one and a 10-bar one isn't cramped into a fixed box.
  height: { type: Number, default: 0 },
})

const container = ref(null)

function colorFor(d) {
  return typeof props.color === 'function' ? props.color(d) : props.color
}

function render() {
  const el = container.value
  if (!el) return
  el.innerHTML = ''
  const width = el.clientWidth || 400
  const data = props.data
  if (!data.length) return

  const rowHeight = 30
  const height = props.height || Math.min(320, Math.max(96, data.length * rowHeight + 32))
  const longestLabel = Math.max(...data.map(d => String(d[props.labelKey]).length))
  const margin = { top: 8, right: 32, bottom: 8, left: Math.min(140, Math.max(48, longestLabel * 6.5 + 12)) }

  const svg = d3.select(el).append('svg')
    .attr('width', width)
    .attr('height', height)

  const y = d3.scaleBand()
    .domain(data.map(d => d[props.labelKey]))
    .range([margin.top, height - margin.bottom])
    .padding(0.25)

  const x = d3.scaleLinear()
    .domain([0, d3.max(data, d => d[props.valueKey]) || 1])
    .range([margin.left, width - margin.right])

  const barThickness = Math.min(24, y.bandwidth())
  const barInset = (y.bandwidth() - barThickness) / 2
  const radius = 4

  // Rounded at the value end, square at the baseline — a plain <rect rx>
  // rounds all four corners, which reads wrong for a bar growing from a fixed edge.
  function barPath(d) {
    const w = Math.max(0, x(d[props.valueKey]) - margin.left)
    const top = y(d[props.labelKey]) + barInset
    const r = Math.min(radius, w, barThickness / 2)
    if (w <= r) {
      return `M${margin.left},${top} h${w} v${barThickness} h${-w} Z`
    }
    return `M${margin.left},${top}
      h${w - r}
      a${r},${r} 0 0 1 ${r},${r}
      v${barThickness - 2 * r}
      a${r},${r} 0 0 1 ${-r},${r}
      h${-(w - r)}
      Z`
  }

  svg.append('g')
    .selectAll('path.bar')
    .data(data)
    .join('path')
    .attr('class', 'bar')
    .attr('d', barPath)
    .attr('fill', d => colorFor(d))
    .style('cursor', 'default')
    .style('transition', 'opacity 0.1s ease')
    .on('mouseenter', function () { d3.select(this).style('opacity', 0.8) })
    .on('mouseleave', function () { d3.select(this).style('opacity', 1) })
    .append('title')
    .text(d => `${d[props.labelKey]}: ${d[props.valueKey]}`)

  svg.append('g')
    .selectAll('text.label')
    .data(data)
    .join('text')
    .attr('class', 'label')
    .attr('x', margin.left - 8)
    .attr('y', d => y(d[props.labelKey]) + y.bandwidth() / 2)
    .attr('dy', '0.32em')
    .attr('text-anchor', 'end')
    .attr('fill', 'var(--hm-text-secondary)')
    .attr('font-size', '11px')
    .text(d => d[props.labelKey])

  svg.append('g')
    .selectAll('text.value')
    .data(data)
    .join('text')
    .attr('class', 'value')
    .attr('x', d => x(d[props.valueKey]) + 6)
    .attr('y', d => y(d[props.labelKey]) + y.bandwidth() / 2)
    .attr('dy', '0.32em')
    .attr('fill', 'var(--hm-text-tertiary)')
    .attr('font-size', '10px')
    .text(d => d[props.valueKey])
}

onMounted(render)
watch(() => props.data, render, { deep: true })
</script>

<template>
  <div ref="container" style="width:100%"></div>
</template>
