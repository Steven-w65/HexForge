import { mount, flushPromises } from '@vue/test-utils'
import { afterEach, expect, it, vi } from 'vitest'

afterEach(() => { vi.restoreAllMocks(); vi.resetModules(); vi.doUnmock('../components/HexCanvas.vue'); vi.unstubAllGlobals() })

it('renders the real initial controls without evaluating the unused canvas, then prewarms after paint', async () => {
  vi.resetModules()
  const evaluated = vi.fn()
  vi.doMock('../components/HexCanvas.vue', () => {
    evaluated()
    return { default: { name: 'HexCanvas', template: '<div data-testid="loaded-canvas" />' } }
  })
  let notify!: PerformanceObserverCallback
  const disconnect = vi.fn()
  vi.stubGlobal('PerformanceObserver', class {
    constructor(callback: PerformanceObserverCallback) { notify = callback }
    observe() {}
    disconnect = disconnect
  })
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  const { default: AppShell } = await import('../components/AppShell.vue')
  const wrapper = mount(AppShell)
  expect(wrapper.get('[data-action="empty-open"]').text()).toBe('Open File')
  expect(wrapper.findAll('[role="menubar"] > button')).toHaveLength(5)
  expect(evaluated).not.toHaveBeenCalled()
  notify({ getEntries: () => [{ name: 'first-contentful-paint' }] } as unknown as PerformanceObserverEntryList, {} as PerformanceObserver)
  await flushPromises()
  expect(evaluated).toHaveBeenCalledOnce()
  expect(disconnect).toHaveBeenCalledOnce()
  await wrapper.setProps({ file: { name: 'a.bin', path: 'a.bin', size: '16', revision: '0', dirty: false } })
  await flushPromises()
  await vi.waitFor(() => expect(wrapper.find('[data-testid="loaded-canvas"]').exists()).toBe(true))
  expect(evaluated).toHaveBeenCalledOnce()
  wrapper.unmount()
})

it('loads the viewer immediately if a file opens before paint', async () => {
  vi.resetModules()
  const evaluated = vi.fn()
  vi.doMock('../components/HexCanvas.vue', () => { evaluated(); return { default: { name: 'HexCanvas', template: '<div data-testid="loaded-canvas" />' } } })
  vi.stubGlobal('PerformanceObserver', class { observe() {} disconnect() {} })
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  const { default: AppShell } = await import('../components/AppShell.vue')
  const wrapper = mount(AppShell)
  await wrapper.setProps({ file: { name: 'a.bin', path: 'a.bin', size: '16', revision: '0', dirty: false } })
  await flushPromises()
  expect(evaluated).toHaveBeenCalledOnce()
  await vi.waitFor(() => expect(wrapper.find('[data-testid="loaded-canvas"]').exists()).toBe(true))
  wrapper.unmount()
})

it('cancels prewarm when the shell closes before paint', async () => {
  vi.resetModules()
  const evaluated = vi.fn()
  vi.doMock('../components/HexCanvas.vue', () => { evaluated(); return { default: { template: '<div />' } } })
  let notify!: PerformanceObserverCallback
  const disconnect = vi.fn()
  vi.stubGlobal('PerformanceObserver', class {
    constructor(callback: PerformanceObserverCallback) { notify = callback }
    observe() {}; disconnect = disconnect
  })
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  const { default: AppShell } = await import('../components/AppShell.vue')
  const wrapper = mount(AppShell)
  wrapper.unmount()
  notify({ getEntries: () => [{ name: 'first-contentful-paint' }] } as unknown as PerformanceObserverEntryList, {} as PerformanceObserver)
  await flushPromises()
  expect(evaluated).not.toHaveBeenCalled()
  expect(disconnect).toHaveBeenCalled()
})

it('keeps controls usable after a failed prewarm and retries on actual file opening', async () => {
  vi.resetModules()
  vi.doMock('../components/HexCanvas.vue', () => { throw new Error('Simulated chunk failure') })
  let notify!: PerformanceObserverCallback
  vi.stubGlobal('PerformanceObserver', class {
    constructor(callback: PerformanceObserverCallback) { notify = callback }
    observe() {}; disconnect() {}
  })
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  const { default: AppShell } = await import('../components/AppShell.vue')
  const wrapper = mount(AppShell)
  notify({ getEntries: () => [{ name: 'first-contentful-paint' }] } as unknown as PerformanceObserverEntryList, {} as PerformanceObserver)
  await flushPromises()
  // Wait for the simulated failed I/O to settle, not just Vue's render queue.
  const { loadHexCanvas } = await import('./hexCanvasLoader')
  await expect(loadHexCanvas()).rejects.toThrow()
  expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  await wrapper.get('[data-action="empty-open"]').trigger('click')
  expect(wrapper.emitted('command')).toEqual([['open']])
  vi.doMock('../components/HexCanvas.vue', () => ({ default: { name: 'HexCanvas', template: '<div data-testid="loaded-canvas" />' } }))
  await wrapper.setProps({ file: { name: 'a.bin', path: 'a.bin', size: '16', revision: '0', dirty: false } })
  await flushPromises()
  await vi.waitFor(() => expect(wrapper.find('[data-testid="loaded-canvas"]').exists()).toBe(true))
  wrapper.unmount()
})

it('reports a required viewer load error with a working retry instead of a blank window', async () => {
  vi.resetModules()
  vi.doMock('../components/HexCanvas.vue', () => { throw new Error('Simulated chunk failure') })
  vi.stubGlobal('PerformanceObserver', class { observe() {}; disconnect() {} })
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  const { default: AppShell } = await import('../components/AppShell.vue')
  const wrapper = mount(AppShell, { props: { file: { name: 'a.bin', path: 'a.bin', size: '16', revision: '0', dirty: false } } })
  await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toContain('Could not load the hex viewer')
  expect(wrapper.emitted('minimap-error')).toHaveLength(1)
  vi.doMock('../components/HexCanvas.vue', () => ({ default: { name: 'HexCanvas', template: '<div data-testid="loaded-canvas" />' } }))
  await wrapper.get('[role="alert"] button').trigger('click')
  await flushPromises()
  await vi.waitFor(() => expect(wrapper.find('[data-testid="loaded-canvas"]').exists()).toBe(true))
  expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  wrapper.unmount()
})
