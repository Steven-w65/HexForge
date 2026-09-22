import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import tauriConfig from '../../src-tauri/tauri.conf.json'
import { COMPACT_LEFT_WIDTH, COMPACT_RIGHT_WIDTH, compactCenterWidth } from '../shell/layout'
import AppShell from './AppShell.vue'

describe('AppShell', () => {
  afterEach(() => vi.restoreAllMocks())
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

  it('blocks Apply and template Save consistently while the editor reports invalid input', async () => {
    const template = { version: 1 as const, name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', offset: '0', type: 'u8' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: { file: { name: 'x', path: 'x', size: '1', revision: '1', dirty: false }, template }, global: { stubs: { HexCanvas: true } } })
    await wrapper.get('input[data-field="offset"]').setValue('0x')
    expect(wrapper.get('[data-action="template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-action="template"]').trigger('click')
    expect(wrapper.emitted('template')).toBeUndefined()
  })

  it('passes orchestration navigation targets into the Canvas viewport', () => {
    const wrapper = mount(AppShell, {
      props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, sourceIdentity: 9, navigationOffset: 160n },
      global: { stubs: { HexCanvas: true } },
    })
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('navigateOffset')).toBe(160n)
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceKey')).toBe('C:/firmware.bin')
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceRevision')).toBe('1')
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceIdentity')).toBe(9)
  })

  it('renders non-blocking operation progress and a truncated-search notice', () => {
    const wrapper = mount(AppShell, { props: {
      busyLabel: 'Searching bytes', progressText: '512 / 1024', searchTruncated: true,
    } })
    expect(wrapper.get('[data-testid="operation-status"]').text()).toContain('Searching bytes')
    expect(wrapper.get('[data-testid="operation-status"]').text()).toContain('512 / 1024')
    expect(wrapper.get('[data-testid="search-truncated"]').text()).toContain('limited')
  })

  it('keeps both row widths reachable across the real configured 1280px shell geometry', async () => {
    let resize!: ResizeObserverCallback
    vi.stubGlobal('ResizeObserver', class {
      constructor(callback: ResizeObserverCallback) { resize = callback }
      observe() {}; disconnect() {}; unobserve() {}
    })
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 1 })
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(function (this: HTMLCanvasElement) {
      return { canvas: this, fillStyle: '', strokeStyle: '', font: '', textBaseline: 'alphabetic', lineWidth: 1, globalAlpha: 1,
        clearRect() {}, fillRect() {}, strokeRect() {}, fillText() {}, drawImage() {}, setTransform() {}, save() {}, restore() {},
      } as unknown as CanvasRenderingContext2D
    })
    const windowWidth = tauriConfig.app.windows[0]!.width
    const centerWidth = compactCenterWidth(windowWidth)
    expect(windowWidth).toBe(1280); expect(centerWidth).toBe(windowWidth - COMPACT_LEFT_WIDTH - COMPACT_RIGHT_WIDTH)
    const wrapper = mount(AppShell, { props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, bytesPerRow: 16 } })
    await nextTick(); await nextTick()
    resize([{ contentRect: { width: centerWidth, height: 500 } } as ResizeObserverEntry], {} as ResizeObserver)
    await nextTick()
    const shellStyle = wrapper.get('[data-testid="app-shell"]').attributes('style') ?? ''
    expect(shellStyle).toContain(`--compact-left-width: ${COMPACT_LEFT_WIDTH}px`)
    expect(shellStyle).toContain(`--compact-right-width: ${COMPACT_RIGHT_WIDTH}px`)
    const viewport = wrapper.get('[data-testid="hex-canvas"]')
    const canvas = wrapper.get('canvas').element as HTMLCanvasElement
    expect(canvas.width + 8).toBeLessThanOrEqual(centerWidth)
    expect((viewport.element as HTMLElement).style.overflowX).toBe('hidden')
    await wrapper.setProps({ bytesPerRow: 32 }); await nextTick()
    expect(canvas.width + 8).toBeGreaterThan(centerWidth)
    expect((viewport.element as HTMLElement).style.overflowX).toBe('auto')
  })
})
