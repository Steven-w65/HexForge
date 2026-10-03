import { mount, flushPromises } from '@vue/test-utils'
import { hexCanvasStub } from '../test/hexCanvasStub'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import tauriConfig from '../../src-tauri/tauri.conf.json'
import type { ParsedNode } from '../types'
import AppShell from './AppShell.vue'

function leaf(name: string, offset: string, length: string, value: string, type: ParsedNode['type'] = 'u8'): ParsedNode {
  return { kind: 'leaf', name, path: name, type, offset, length, value, endianness: 'little', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] }
}

describe('AppShell', () => {
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); localStorage.clear() })

  it('uses observer measurements without forcing workspace layout during mount', async () => {
    let callback!: ResizeObserverCallback
    let observed!: Element
    const disconnect = vi.fn()
    vi.stubGlobal('ResizeObserver', class {
      constructor(handler: ResizeObserverCallback) { callback = handler }
      observe(element: Element) { observed = element }
      disconnect = disconnect
    })
    const measure = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect')
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub, ParsedResultsPanel: true } } })
    expect(observed).toBe(wrapper.get('.workspace').element)
    expect(measure).not.toHaveBeenCalled()
    callback([{ target: observed, contentRect: { height: 300 } } as ResizeObserverEntry], {} as ResizeObserver)
    await nextTick()
    expect(wrapper.get('[data-testid="app-shell"]').attributes('style')).toContain('--results-pane-height: 136px')
    expect(measure).not.toHaveBeenCalled()
    wrapper.unmount()
    expect(disconnect).toHaveBeenCalledOnce()
  })

  it('coalesces fallback layout measurements and cancels the pending frame on unmount', async () => {
    vi.stubGlobal('ResizeObserver', undefined)
    let measureFrame!: FrameRequestCallback
    const request = vi.fn((callback: FrameRequestCallback) => { measureFrame = callback; return 27 })
    const cancel = vi.fn()
    vi.stubGlobal('requestAnimationFrame', request)
    vi.stubGlobal('cancelAnimationFrame', cancel)
    const measure = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ height: 350 } as DOMRect)
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub, ParsedResultsPanel: true } } })
    expect(measure).not.toHaveBeenCalled()
    window.dispatchEvent(new Event('resize')); window.dispatchEvent(new Event('resize'))
    expect(request).toHaveBeenCalledOnce()
    measureFrame(0); await nextTick()
    expect(measure).toHaveBeenCalledOnce()
    expect(wrapper.get('[data-testid="app-shell"]').attributes('style')).toContain('--results-pane-height: 186px')
    window.dispatchEvent(new Event('resize'))
    wrapper.unmount()
    expect(cancel).toHaveBeenCalledWith(27)
    measureFrame(0)
    expect(measure).toHaveBeenCalledOnce()
  })
  it('opens from the centered no-file prompt through the shared command route', async () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(Array.from(wrapper.get('[data-testid="drop-prompt"]').element.children, child => child.textContent?.trim())).toEqual([
      '＋', 'Drop a binary file', 'Open File',
    ])
    const button = wrapper.get('[data-testid="drop-prompt"] [data-action="empty-open"]')
    expect(button.text()).toBe('Open File')
    expect(button.attributes('title')).toContain('Ctrl+O')
    await button.trigger('click')
    expect(wrapper.emitted('command')).toEqual([['open']])
    expect(wrapper.get('[data-testid="file-info-bar"]').text()).toContain('No file open')
    expect(wrapper.findAll('[role="menubar"] > button').map((item) => item.text())).toEqual(['File', 'Edit', 'Navigate', 'Template', 'View'])
    expect(wrapper.find('.status-bar').exists()).toBe(true)
    wrapper.unmount()
  })

  it('disables empty-state Open while busy and enables it again when the operation finishes', async () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub } } })
    const state = wrapper.props('menuState')!
    await wrapper.setProps({ menuState: { ...state, operationBusy: true } })
    const button = wrapper.get('[data-action="empty-open"]')
    expect(button.attributes('disabled')).toBeDefined()
    expect(button.attributes('title')).toContain('Wait for the current operation')
    await button.trigger('click')
    expect(wrapper.emitted('command')).toBeUndefined()
    await wrapper.setProps({ menuState: { ...state, operationBusy: false } })
    expect(button.attributes('disabled')).toBeUndefined()
    await button.trigger('click')
    expect(wrapper.emitted('command')).toEqual([['open']])
    wrapper.unmount()
  })

  it('does not cover an opened file with the drop prompt when search is closed', () => {
    const wrapper = mount(AppShell, {
      props: { file: { name: 'image.bin', path: 'C:/image.bin', size: '16', revision: '1', dirty: false } },
      global: { stubs: { HexCanvas: hexCanvasStub } },
    })

    expect(wrapper.find('[data-testid="drop-prompt"]').exists()).toBe(false)
    expect(wrapper.find('[data-action="empty-open"]').exists()).toBe(false)
    expect(wrapper.findComponent({ name: 'HexCanvas' }).exists()).toBe(true)
  })

  it('keeps the menu row at its compact height', () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub } } })
    const style = wrapper.get('[data-testid="app-shell"]').attributes('style') ?? ''
    expect(style).toContain('--top-menu-height: 34px')
  })

  it('suppresses the browser context menu inside the main window', () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub } } })
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    wrapper.get('[data-testid="file-info-bar"]').element.dispatchEvent(event)
    wrapper.unmount()
    expect(event.defaultPrevented).toBe(true)
  })

  it('requests the bottom results-pane toggle and row-width changes', async () => {
    const wrapper = mount(AppShell, { props: { rightCollapsed: false } })
    await wrapper.get('[data-action="collapse-results"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['toggle-right-panel']])
    await wrapper.setProps({ rightCollapsed: true })
    expect(wrapper.get('[data-testid="app-shell"]').classes()).toContain('results-collapsed')
    expect(wrapper.get('.workspace').element.lastElementChild?.classList.contains('results-panel')).toBe(true)
    await wrapper.get('[data-row-width="32"]').trigger('click')
    expect(wrapper.emitted('update:bytesPerRow')).toEqual([[32]])
  })

  it('passes the canvas template highlight to the matching parsed result row', () => {
    const wrapper = mount(AppShell, { props: {
      file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false },
      templateSource: 'file', templateApplied: true,
      results: [{ ...leaf('size', '16', '4', '640', 'u32'), endianness: 'big' }],
      templateRange: { start: 16n, end: 19n, count: 4n },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(wrapper.get('[data-testid="parsed-result"]').classes()).toContain('active')
  })

  it('replaces shortcut buttons with collapsible file information in the second row', async () => {
    const wrapper = mount(AppShell, {
      props: {
        file: { name: 'firmware.bin', path: 'C:/projects/very/long/path/firmware.bin', size: '45103137', revision: '1', dirty: true },
        menuState: { hasFile: true, hasBytes: true, singleByteSelected: true, editMode: false, canUndo: true, templateValid: true, templateActive: true, templateHasPath: true, templateHasFields: true, hasParsedResults: true, operationBusy: false },
        template: { name: 'T', defaultEndianness: 'little', fields: [{ name: 'x', placement: { mode: 'absolute', offset: '0' }, type: 'u8', comment: '' }] },
        results: [leaf('x', '0', '1', '41')],
      },
      global: { stubs: { HexCanvas: { template: '<div data-testid="canvas-placeholder" />' } } },
    })
    expect(wrapper.findAll('[data-action="open"], [data-action="search"], [data-action="template"], [data-action="edit"], [data-action="save-as"]')).toHaveLength(0)
    const bar = wrapper.get('[data-testid="file-info-bar"]')
    expect(bar.text()).toContain('firmware.bin')
    expect(bar.text()).toContain('43.01 MiB')
    expect(bar.text()).toContain('Modified')
    expect(wrapper.get('.menu-brand').text()).toBe('HFHexForge')
    expect(wrapper.get('[data-testid="file-path"]').attributes('title')).toBe('C:/projects/very/long/path/firmware.bin')
    expect(wrapper.find('[data-testid="file-details"]').exists()).toBe(false)
    await wrapper.get('[data-action="toggle-file-details"]').trigger('click')
    expect(wrapper.get('[data-action="toggle-file-details"]').attributes('aria-expanded')).toBe('true')
    expect(wrapper.get('[data-testid="file-details"]').text()).toContain('45103137 bytes')
    expect(wrapper.get('[data-testid="file-details"]').text()).toContain('C:/projects/very/long/path/firmware.bin')
  })

  it('keeps file information out of the workspace and reserves its bottom row for results', async () => {
    const wrapper = mount(AppShell)
    expect(wrapper.find('.sidebar-panel').exists()).toBe(false)
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(false)
    expect(wrapper.get('.workspace').element.lastElementChild?.classList.contains('results-panel')).toBe(true)
  })

  it('keeps file details above the hex and results workspace', async () => {
    const wrapper = mount(AppShell, { props: {
      file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    await wrapper.get('[data-action="toggle-file-details"]').trigger('click')
    expect(wrapper.find('[data-testid="file-details"]').exists()).toBe(true)
    expect(wrapper.get('.workspace').find('[data-testid="template-editor-panel"]').exists()).toBe(false)
  })

  it('keeps the selected offset visible when its byte is outside the current page', () => {
    const wrapper = mount(AppShell, { props: {
      file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false },
      page: { offset: '0', bytes: [0x41], modifiedOffsets: [], revision: '1', generation: 1 },
      selection: { start: 100n, end: 100n, count: 1n },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(wrapper.get('.status-bar').text()).toContain('Offset 0x64')
    expect(wrapper.get('.status-bar').text()).toContain('Byte —')
  })

  it('replaces the Open prompt with an empty-file message even for a zero-byte file', async () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(wrapper.find('[data-action="empty-open"]').exists()).toBe(true)
    await wrapper.setProps({ file: { name: 'empty.bin', path: 'C:/empty.bin', size: '0', revision: '1', dirty: false } })
    expect(wrapper.find('[data-action="empty-open"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="empty-file"]').text()).toContain('empty')
  })

  it('forwards menu commands and dynamic navigation ranges', async () => {
    const template = { name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', placement: { mode: 'absolute' as const, offset: '4' }, type: 'u16' as const, comment: '' }] }
    const wrapper = mount(AppShell, {
      props: {
        file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false }, template,
        results: [leaf('x', '4', '2', '123', 'u16')],
        menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: true, templateHasPath: true, templateHasFields: true, hasParsedResults: true, operationBusy: false },
      },
      global: { stubs: { HexCanvas: hexCanvasStub } },
    })
    await wrapper.get('[data-menu="template"]').trigger('click')
    await wrapper.get('[data-menu-command="load-template"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['load-template']])
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    await wrapper.get('[data-submenu="parsed-results"]').trigger('click')
    await wrapper.get('[data-parsed-result="0"]').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 4n, end: 5n }]])
  })

  it('blocks Apply and Save consistently when the editor reports invalid input', async () => {
    const template = { name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', placement: { mode: 'absolute' as const, offset: '0' }, type: 'u8' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: { file: { name: 'x', path: 'x', size: '1', revision: '1', dirty: false }, template,
      templateValid: false, menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: false, templateActive: true, templateHasPath: true, templateHasFields: true, hasParsedResults: false, operationBusy: false },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-menu-command="save-template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-menu-command="save-template"]').attributes('title')).toContain('Correct')
    await wrapper.get('[data-menu-command="apply-template"]').trigger('click')
    expect(wrapper.emitted('command')).toBeUndefined()
  })

  it('routes Apply from the Template menu without mounting the editor', async () => {
    const template = { name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', placement: { mode: 'absolute' as const, offset: '0' }, type: 'u8' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: {
      file: { name: 'x.bin', path: 'C:/x.bin', size: '1', revision: '1', dirty: false }, template,
      menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: true, templateHasPath: true, templateHasFields: true, hasParsedResults: false, operationBusy: false },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(wrapper.find('[data-testid="template-modified"]').exists()).toBe(false)
    await wrapper.get('[data-menu="template"]').trigger('click')
    await wrapper.get('[data-menu-command="apply-template"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['apply-template']])
  })

  it('forwards a pending loaded-template action from the bottom panel through the existing command route', async () => {
    const template = { name: 'Internal title', defaultEndianness: 'little' as const, fields: [{ name: 'x', placement: { mode: 'absolute' as const, offset: '0' }, type: 'u8' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: {
      file: { name: 'x.bin', path: 'C:/x.bin', size: '1', revision: '1', dirty: false }, template,
      templateSource: 'file', templateDisplayName: 'header.json', templateApplied: false,
      menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: true, templateHasPath: true, templateHasFields: true, hasParsedResults: false, operationBusy: false },
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Template: "header.json" loaded, pending apply.')
    await wrapper.get('[data-action="results-apply-template"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['apply-template']])
    await wrapper.setProps({ templateApplied: true })
    expect(wrapper.find('.result-list').exists()).toBe(true)
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
  })

  it('passes orchestration navigation targets into the Canvas viewport', () => {
    const wrapper = mount(AppShell, {
      props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, sourceIdentity: 9, navigationOffset: 160n },
      global: { stubs: { HexCanvas: hexCanvasStub } },
    })
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('navigateOffset')).toBe(160n)
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceKey')).toBe('C:/firmware.bin')
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceRevision')).toBe('1')
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('sourceIdentity')).toBe(9)
  })

  it('marks only the active parsed range in the minimap and preserves edit markers', async () => {
    const field = leaf('magic', '32', '4', '89 50 4E 47', 'bytes')
    const overview = { binCount: 1024, bins: [0, 512] }
    const wrapper = mount(AppShell, {
      props: { file: { name: 'image.bin', path: 'C:/image.bin', size: '4096', revision: '2', dirty: true },
        results: [field, leaf('size', '36', '4', '640', 'u32')], modifiedOverview: overview, matches: [128n] },
      global: { stubs: { HexCanvas: hexCanvasStub } },
    })

    const canvas = wrapper.findComponent({ name: 'HexCanvas' })
    expect(canvas.props('templateFields')).toEqual([])
    await wrapper.setProps({ templateRange: { start: 32n, end: 35n, count: 4n } })
    expect(canvas.props('templateFields')).toEqual([{ name: 'magic', path: 'magic', offset: '32', length: 4, type: 'bytes', value: '89 50 4E 47' }])
    await wrapper.setProps({ templateRange: { start: 36n, end: 39n, count: 4n } })
    expect(canvas.props('templateFields')).toEqual([{ name: 'size', path: 'size', offset: '36', length: 4, type: 'u32', value: '640' }])
    await wrapper.setProps({ templateRange: null })
    expect(canvas.props('templateFields')).toEqual([])
    expect(canvas.props('modifiedOverview')).toEqual(overview)
    expect(canvas.props('matches')).toEqual([128n])
    wrapper.unmount()
  })

  it('matches nested parsed ranges exactly above the JavaScript safe-integer limit', () => {
    const field = { ...leaf('id', '9007199254740993', '8', '18446744073709551615', 'u64'), path: 'header.id' }
    const header: ParsedNode = { ...leaf('header', '9007199254740992', '9', ''), kind: 'struct', type: 'struct', value: null,
      children: [leaf('tag', '9007199254740992', '1', '1'), field] }
    const wrapper = mount(AppShell, {
      props: { file: { name: 'large.bin', path: 'C:/large.bin', size: '9007199254741001', revision: '1', dirty: false }, results: [header],
        templateRange: { start: 9007199254740993n, end: 9007199254741000n, count: 8n } },
      global: { stubs: { HexCanvas: hexCanvasStub } },
    })
    expect(wrapper.findComponent({ name: 'HexCanvas' }).props('templateFields')).toEqual([
      { name: 'id', path: 'header.id', offset: '9007199254740993', length: 8, type: 'u64', value: '18446744073709551615' },
    ])
    wrapper.unmount()
  })

  it('renders operation progress in the status bar and a truncated-search notice', () => {
    const wrapper = mount(AppShell, { props: {
      busyLabel: 'Searching bytes', progressText: '512 / 1024', searchTruncated: true,
    } })
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('Searching bytes')
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('512 / 1024')
    expect(wrapper.find('[data-testid="operation-status"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="search-truncated"]').text()).toContain('limited')
  })

  it('fits 16-byte rows beside the minimap and scrolls 32-byte rows when needed', async () => {
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
    expect(windowWidth).toBe(1280)
    const wrapper = mount(AppShell, { props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, bytesPerRow: 16 } })
    await flushPromises(); await nextTick()
    await vi.waitFor(() => expect(wrapper.find('[data-testid="hex-canvas"]').exists()).toBe(true))
    // ResizeObserver now observes the dedicated editor column, excluding the
    // 84px minimap and 12px overview ruler siblings.
    const editorWidth = windowWidth - 96
    resize([{ contentRect: { width: editorWidth, height: 500 } } as ResizeObserverEntry], {} as ResizeObserver)
    await nextTick()
    const shellStyle = wrapper.get('[data-testid="app-shell"]').attributes('style') ?? ''
    expect(shellStyle).toContain('--results-pane-height: 220px')
    const viewport = wrapper.get('[data-testid="hex-canvas"]')
    const canvas = wrapper.get('canvas').element as HTMLCanvasElement
    expect(canvas.width).toBeLessThanOrEqual(editorWidth)
    expect((viewport.element as HTMLElement).style.overflowX).toBe('hidden')
    await wrapper.setProps({ bytesPerRow: 32 }); await nextTick()
    expect(canvas.width).toBeGreaterThan(editorWidth)
    expect((viewport.element as HTMLElement).style.overflowX).toBe('auto')
  })

  it('passes minimap settings and edit deltas to Canvas and forwards nonfatal errors', async () => {
    const settings = { enabled: false, mode: 'proportional' as const, renderCharacters: false, scale: 2 as const }
    const delta = { offset: 8n, revision: '2' }
    const wrapper = mount(AppShell, { props: {
      file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '2', dirty: true },
      minimapSettings: settings, editDelta: delta,
    }, global: { stubs: { HexCanvas: hexCanvasStub } } })
    const canvas = wrapper.findComponent({ name: 'HexCanvas' })
    expect(canvas.props('minimapSettings')).toEqual(settings)
    expect(canvas.props('editDelta')).toEqual(delta)
    expect(wrapper.get('[data-testid="app-shell"]').attributes('style')).toContain('--minimap-width: 12px')
    canvas.vm.$emit('minimap-error', new Error('Sample failed'))
    expect(wrapper.emitted('minimap-error')).toEqual([[expect.any(Error)]])
  })
})
