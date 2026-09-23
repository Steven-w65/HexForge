import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import tauriConfig from '../../src-tauri/tauri.conf.json'
import { COMPACT_RIGHT_WIDTH, compactCenterWidth } from '../shell/layout'
import AppShell from './AppShell.vue'

describe('AppShell', () => {
  afterEach(() => vi.restoreAllMocks())
  it('shows a centered no-file drop prompt while keeping Open enabled', () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: true } } })
    expect(wrapper.get('[data-testid="drop-prompt"]').text()).toContain('Open or drop a binary file to begin')
    expect(wrapper.find('[data-action="empty-open"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="file-info-bar"]').text()).toContain('No file open')
    expect(wrapper.findAll('[role="menubar"] > button').map((item) => item.text())).toEqual(['File', 'Edit', 'Navigate', 'Template', 'View'])
    expect(wrapper.find('.status-bar').exists()).toBe(true)
  })

  it('requests the parsed-results panel toggle and row-width changes', async () => {
    const wrapper = mount(AppShell, { props: { rightCollapsed: false } })
    await wrapper.get('[data-action="collapse-right"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['toggle-right-panel']])
    await wrapper.setProps({ rightCollapsed: true })
    expect(wrapper.get('[data-testid="app-shell"]').classes()).toContain('right-collapsed')
    await wrapper.get('[data-row-width="32"]').trigger('click')
    expect(wrapper.emitted('update:bytesPerRow')).toEqual([[32]])
  })

  it('replaces shortcut buttons with collapsible file information in the second row', async () => {
    const wrapper = mount(AppShell, {
      props: {
        file: { name: 'firmware.bin', path: 'C:/projects/very/long/path/firmware.bin', size: '45103137', revision: '1', dirty: true },
        menuState: { hasFile: true, hasBytes: true, singleByteSelected: true, editMode: false, canUndo: true, templateValid: true, templateHasFields: true, hasNavigableTemplateFields: true, hasParsedResults: true, operationBusy: false },
        template: { version: 1, name: 'T', defaultEndianness: 'little', fields: [{ name: 'x', offset: '0', type: 'u8', comment: '' }] },
        results: [{ name: 'x', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }],
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

  it('keeps file information out of the workspace and mounts the Template Editor only in its separate sheet', async () => {
    const wrapper = mount(AppShell, { props: { templateEditorOpen: false } })
    expect(wrapper.find('.sidebar-panel').exists()).toBe(false)
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(false)
    await wrapper.setProps({ templateEditorOpen: true })
    expect(wrapper.find('[data-testid="template-editor-panel"] .template-editor').exists()).toBe(true)
    await wrapper.get('[data-action="close-template-editor"]').trigger('click')
    expect(wrapper.emitted('close-template-editor')).toEqual([[]])
  })

  it('routes the empty-state Open action and identifies an opened empty file', async () => {
    const wrapper = mount(AppShell, { global: { stubs: { HexCanvas: true } } })
    await wrapper.get('[data-action="empty-open"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['open']])
    await wrapper.setProps({ file: { name: 'empty.bin', path: 'C:/empty.bin', size: '0', revision: '1', dirty: false } })
    expect(wrapper.get('[data-testid="empty-file"]').text()).toContain('empty')
  })

  it('forwards menu commands and dynamic navigation ranges', async () => {
    const template = { version: 1 as const, name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', offset: '4', type: 'u16' as const, comment: '' }] }
    const wrapper = mount(AppShell, {
      props: {
        file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false }, template,
        menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateHasFields: true, hasNavigableTemplateFields: true, hasParsedResults: false, operationBusy: false },
      },
      global: { stubs: { HexCanvas: true } },
    })
    await wrapper.get('[data-menu="template"]').trigger('click')
    await wrapper.get('[data-menu-command="load-template"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['load-template']])
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    await wrapper.get('[data-submenu="template-fields"]').trigger('click')
    await wrapper.get('[data-template-field="0"]').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 4n, end: 5n }]])
  })

  it('blocks Apply and template Save consistently while the editor reports invalid input', async () => {
    const template = { version: 1 as const, name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', offset: '0', type: 'u8' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: { file: { name: 'x', path: 'x', size: '1', revision: '1', dirty: false }, template, templateEditorOpen: true }, global: { stubs: { HexCanvas: true } } })
    await wrapper.get('input[data-field="offset"]').setValue('0x')
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-menu-command="apply-template"]').trigger('click')
    expect(wrapper.emitted('command')).toBeUndefined()
  })

  it('routes Apply from the Template Editor and shows template modified state', async () => {
    const template = { version: 1 as const, name: 'T', defaultEndianness: 'little' as const, fields: [{ name: 'x', offset: '0', type: 'u8' as const, comment: '' }] }
    const wrapper = mount(AppShell, { props: {
      file: { name: 'x.bin', path: 'C:/x.bin', size: '1', revision: '1', dirty: false }, template,
      templateEditorOpen: true, templateDirty: true,
      menuState: { hasFile: true, hasBytes: true, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateHasFields: true, hasNavigableTemplateFields: true, hasParsedResults: false, operationBusy: false },
    }, global: { stubs: { HexCanvas: true } } })
    expect(wrapper.get('[data-testid="template-modified"]').text()).toContain('Modified')
    await wrapper.get('[data-action="apply-template"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['apply-template']])
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

  it('renders operation progress in the status bar and a truncated-search notice', () => {
    const wrapper = mount(AppShell, { props: {
      busyLabel: 'Searching bytes', progressText: '512 / 1024', searchTruncated: true,
    } })
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('Searching bytes')
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('512 / 1024')
    expect(wrapper.find('[data-testid="operation-status"]').exists()).toBe(false)
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
    expect(windowWidth).toBe(1280); expect(centerWidth).toBe(1040)
    const wrapper = mount(AppShell, { props: { file: { name: 'firmware.bin', path: 'C:/firmware.bin', size: '4096', revision: '1', dirty: false }, bytesPerRow: 16 } })
    await nextTick(); await nextTick()
    resize([{ contentRect: { width: centerWidth, height: 500 } } as ResizeObserverEntry], {} as ResizeObserver)
    await nextTick()
    const shellStyle = wrapper.get('[data-testid="app-shell"]').attributes('style') ?? ''
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
