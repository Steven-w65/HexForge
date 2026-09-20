import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import AppShell from './AppShell.vue'

describe('AppShell', () => {
  it('shows a centered no-file drop prompt while keeping Open enabled', () => {
    const wrapper = mount(AppShell)
    expect(wrapper.get('[data-testid="drop-prompt"]').text()).toContain('Drop a binary file')
    expect(wrapper.get('[data-action="open"]').attributes('disabled')).toBeUndefined()
  })

  it('collapses both side panels and toggles between 16 and 32 bytes', async () => {
    const wrapper = mount(AppShell)
    await wrapper.get('[data-action="collapse-left"]').trigger('click')
    await wrapper.get('[data-action="collapse-right"]').trigger('click')
    expect(wrapper.get('[data-testid="app-shell"]').classes()).toContain('left-collapsed')
    expect(wrapper.get('[data-testid="app-shell"]').classes()).toContain('right-collapsed')
    await wrapper.get('[data-row-width="32"]').trigger('click')
    expect(wrapper.emitted('update:bytesPerRow')).toEqual([[32]])
  })

  it('forwards every toolbar action', async () => {
    const wrapper = mount(AppShell, {
      props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false } },
      global: { stubs: { HexCanvas: { template: '<div data-testid="canvas-placeholder" />' } } },
    })
    for (const action of ['open', 'goto', 'search', 'template', 'export', 'theme', 'edit', 'save-as']) {
      await wrapper.get(`[data-action="${action}"]`).trigger('click')
      expect(wrapper.emitted(action)).toHaveLength(1)
    }
  })

  it('passes orchestration navigation targets into the Canvas viewport', () => {
    const wrapper = mount(AppShell, {
      props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, navigationOffset: 160n },
      global: { stubs: { HexCanvas: true } },
    })
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('navigateOffset')).toBe(160n)
  })

  it('renders non-blocking operation progress and a truncated-search notice', () => {
    const wrapper = mount(AppShell, { props: {
      busyLabel: 'Searching bytes', progressText: '512 / 1024', searchTruncated: true,
    } })
    expect(wrapper.get('[data-testid="operation-status"]').text()).toContain('Searching bytes')
    expect(wrapper.get('[data-testid="operation-status"]').text()).toContain('512 / 1024')
    expect(wrapper.get('[data-testid="search-truncated"]').text()).toContain('limited')
  })
})
