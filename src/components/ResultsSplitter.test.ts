import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AppShell from './AppShell.vue'

describe('bottom results resizing', () => {
  afterEach(() => { vi.restoreAllMocks(); localStorage.clear() })
  function mountShell(collapsed = false) {
    const wrapper = mount(AppShell, { props: { rightCollapsed: collapsed }, global: { stubs: { HexCanvas: true } } })
    vi.spyOn(wrapper.get('.workspace').element, 'getBoundingClientRect').mockReturnValue({ top: 60, bottom: 660, height: 600 } as DOMRect)
    window.dispatchEvent(new Event('resize'))
    return wrapper
  }
  it('resizes with keyboard, clamps to preserve the canvas, and restores the preferred size', async () => {
    const wrapper = mountShell()
    const divider = wrapper.get('[role="separator"]')
    await divider.trigger('keydown', { key: 'ArrowUp' })
    expect(divider.attributes('aria-valuenow')).toBe('240')
    await divider.trigger('keydown', { key: 'Home' })
    expect(divider.attributes('aria-valuenow')).toBe('84')
    await divider.trigger('keydown', { key: 'End' })
    expect(divider.attributes('aria-valuenow')).toBe('436')
    expect(localStorage.getItem('hexforge.resultsPaneHeight')).toBe('436')
    wrapper.unmount()
    const reopened = mountShell()
    expect(reopened.get('[role="separator"]').attributes('aria-valuenow')).toBe('436')
    reopened.unmount()
  })
  it('keeps pointer capture while dragging and clamps movement beyond the viewport', async () => {
    const wrapper = mountShell()
    const divider = wrapper.get('[role="separator"]')
    const capture = vi.fn(), release = vi.fn()
    Object.defineProperties(divider.element, { setPointerCapture: { value: capture }, releasePointerCapture: { value: release }, hasPointerCapture: { value: () => true } })
    await divider.trigger('pointerdown', { clientY: 440, pointerId: 1, button: 0 })
    await divider.trigger('pointermove', { clientY: 400, pointerId: 1 })
    expect(divider.attributes('aria-valuenow')).toBe('260')
    await divider.trigger('pointermove', { clientY: -1000, pointerId: 1 })
    expect(divider.attributes('aria-valuenow')).toBe('436')
    await divider.trigger('pointerup', { pointerId: 1 })
    expect(capture).toHaveBeenCalledWith(1)
    expect(release).toHaveBeenCalledWith(1)
    wrapper.unmount()
  })
  it('removes the splitter when collapsed and recovers the expanded height', async () => {
    const wrapper = mountShell()
    await wrapper.get('[role="separator"]').trigger('keydown', { key: 'ArrowUp' })
    await wrapper.setProps({ rightCollapsed: true })
    expect(wrapper.find('[role="separator"]').exists()).toBe(false)
    await wrapper.setProps({ rightCollapsed: false })
    expect(wrapper.get('[role="separator"]').attributes('aria-valuenow')).toBe('240')
    wrapper.unmount()
  })
})
