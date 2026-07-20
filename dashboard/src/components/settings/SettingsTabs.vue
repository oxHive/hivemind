<script setup>
defineProps({ tabs: { type: Array, required: true }, active: { type: String, required: true } })
defineEmits(['select'])
</script>

<template>
  <div class="tabs-bar mb-8" role="tablist">
    <button v-for="tab in tabs" :key="tab.id"
      role="tab"
      :aria-selected="active === tab.id"
      class="tab-btn"
      :class="{ 'tab-btn-active': active === tab.id }"
      @click="$emit('select', tab.id)">
      {{ tab.label }}
    </button>
  </div>
</template>

<style scoped>
.tabs-bar {
  display: flex;
  gap: 22px;
  border-bottom: 0.5px solid var(--hm-border-subtle);
}

.tab-btn {
  position: relative;
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0 0 10px;
  font-family: var(--hm-font-mono);
  font-size: var(--hm-text-sm);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--hm-text-tertiary);
  transition: color 0.12s;
}

.tab-btn:hover {
  color: var(--hm-text-secondary);
}

.tab-btn:focus-visible {
  outline: 2px solid var(--hm-accent);
  outline-offset: 2px;
}

.tab-btn::after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  bottom: -0.5px;
  height: 2px;
  background: var(--hm-accent);
  transform: scaleX(0);
  transition: transform 0.12s;
}

.tab-btn-active {
  color: var(--hm-text-primary);
}

.tab-btn-active::after {
  transform: scaleX(1);
}
</style>
