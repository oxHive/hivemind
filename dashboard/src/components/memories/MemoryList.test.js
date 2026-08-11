import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import MemoryList from './MemoryList.vue'

describe('MemoryList', () => {
  it('includes an org filter chip', () => {
    setActivePinia(createPinia())
    const wrapper = mount(MemoryList, {
      global: { stubs: { TagFilter: true, MemoryCard: true, SkeletonCard: true } },
    })
    expect(wrapper.text()).toContain('org')
  })
})
