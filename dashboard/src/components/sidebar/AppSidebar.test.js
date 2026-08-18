import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AppSidebar from './AppSidebar.vue'
import { useUiStore } from '../../stores/ui.js'
import { useFeedbackStore } from '../../stores/feedback.js'
import { useGraphStore } from '../../stores/graph.js'

describe('AppSidebar header and brand footer (via @oxhive/ui AppSidebar)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('passes the HiveMind product name and server version to the header', () => {
    const ui = useUiStore()
    ui.serverInfo = { version: '0.14.3' }
    const wrapper = mount(AppSidebar)
    expect(wrapper.text()).toContain('HiveMind')
    expect(wrapper.text()).toContain('v0.14.3')
  })

  it('always renders the OxHive brand footer', () => {
    const wrapper = mount(AppSidebar)
    expect(wrapper.text()).toContain('OxHive')
    expect(wrapper.find('img.oxui-sidebar__brand-mark').exists()).toBe(true)
  })
})

describe('AppSidebar org status row', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('shows an org status row when org is configured', () => {
    const ui = useUiStore()
    ui.orgInfo = { configured: true, enabled: true, last_synced_at: null, conflict_count: 0 }
    const wrapper = mount(AppSidebar)
    expect(wrapper.text()).toContain('org')
  })

  it('does not show an org status row when org is not configured', () => {
    const ui = useUiStore()
    ui.orgInfo = null
    const wrapper = mount(AppSidebar)
    expect(wrapper.text()).not.toContain('org')
  })
})

describe('AppSidebar nav (via @oxhive/ui AppNav)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('renders all 5 nav items with labels and icon components', () => {
    const wrapper = mount(AppSidebar)
    const labels = ['Analytics', 'Memories', 'Graph', 'Feedback', 'Settings']
    for (const label of labels) {
      expect(wrapper.text()).toContain(label)
    }
    // each nav item's icon renders as an actual <svg>, not a string
    expect(wrapper.findAll('.oxui-nav-item svg').length).toBe(5)
  })

  it('marks the active view item with aria-current and active class', () => {
    const ui = useUiStore()
    ui.activeView = 'graph'
    const wrapper = mount(AppSidebar)
    const buttons = wrapper.findAll('.oxui-nav-item')
    const graphBtn = buttons.find(b => b.text().includes('Graph'))
    expect(graphBtn.attributes('aria-current')).toBe('page')
    expect(graphBtn.classes()).toContain('oxui-nav-item--active')
  })

  it('calls ui.requestActiveView when a nav item is clicked', async () => {
    const ui = useUiStore()
    ui.activeView = 'analytics'
    const wrapper = mount(AppSidebar)
    const buttons = wrapper.findAll('.oxui-nav-item')
    const settingsBtn = buttons.find(b => b.text().includes('Settings'))
    await settingsBtn.trigger('click')
    expect(ui.activeView).toBe('settings')
  })

  it('shows a badge on Feedback when there are conflicts or feedback items', () => {
    const feedback = useFeedbackStore()
    feedback.conflicts = [{ id: 1 }]
    feedback.feedbackItems = [{ id: 2 }, { id: 3 }]
    const wrapper = mount(AppSidebar)
    const buttons = wrapper.findAll('.oxui-nav-item')
    const feedbackBtn = buttons.find(b => b.text().includes('Feedback'))
    expect(feedbackBtn.find('.oxui-nav-item__badge').text()).toBe('3')
  })

  it('shows a badge on Memories and Graph when there are pending edges', () => {
    const graph = useGraphStore()
    graph.edges = [{ id: 'e1', status: 'pending' }]
    const wrapper = mount(AppSidebar)
    const buttons = wrapper.findAll('.oxui-nav-item')
    const memoriesBtn = buttons.find(b => b.text().includes('Memories'))
    const graphBtn = buttons.find(b => b.text().includes('Graph'))
    expect(memoriesBtn.find('.oxui-nav-item__badge').text()).toBe('1')
    expect(graphBtn.find('.oxui-nav-item__badge').text()).toBe('1')
  })
})
