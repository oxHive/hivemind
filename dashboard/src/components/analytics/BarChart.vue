<script setup>
import { ref, onMounted, watch } from 'vue'
import * as d3 from 'd3'

const props = defineProps({
  data: { type: Array, required: true },
  labelKey: { type: String, required: true },
  valueKey: { type: String, required: true },
  color: { type: [String, Function], default: 'var(--hm-accent)' },
  height: { type: Number, default: 180 },
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
  const height = props.height
  const margin = { top: 8, right: 12, bottom: 24, left: 90 }

  const svg = d3.select(el).append('svg')
    .attr('width', width)
    .attr('height', height)

  const data = props.data
  if (!data.length) return

  const y = d3.scaleBand()
    .domain(data.map(d => d[props.labelKey]))
    .range([margin.top, height - margin.bottom])
    .padding(0.25)

  const x = d3.scaleLinear()
    .domain([0, d3.max(data, d => d[props.valueKey]) || 1])
    .range([margin.left, width - margin.right])

  svg.append('g')
    .selectAll('rect')
    .data(data)
    .join('rect')
    .attr('x', margin.left)
    .attr('y', d => y(d[props.labelKey]))
    .attr('width', d => x(d[props.valueKey]) - margin.left)
    .attr('height', y.bandwidth())
    .attr('fill', d => colorFor(d))
    .attr('rx', 2)

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
