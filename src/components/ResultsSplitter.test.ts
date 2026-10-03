import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AppShell from './AppShell.vue'

describe('bottom results resizing', () => {
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); localStorage.clear() })
  function mountShell(collapsed = false) {
    let measured!: ResizeObserverCallback
    vi.stubGlobal('ResizeObserver', class {
      constructor(callback: ResizeObserverCallback) { measured = callback }
      observe() {}; disconnect() {}
    })
    const wrapper = mount(AppShell, { props: { rightCollapsed: collapsed }, global: { stubs: { HexCanvas: true } } })
    // Deliver the browser's completed layout instead of forcing synchronous
    // measurement from a resize event. The resulting geometry is unchanged.
    measured([{ contentRect: { height: 600 } } as ResizeObserverEntry], {} as ResizeObserver)
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
