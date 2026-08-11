import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import GraphToolbar from './GraphToolbar.vue'

describe('GraphToolbar', () => {
  it('includes an Org layer tab', () => {
    setActivePinia(createPinia())
    const wrapper = mount(GraphToolbar, {
      global: { stubs: { TagFilter: true } },
    })
    expect(wrapper.text()).toContain('Org')
  })
})
