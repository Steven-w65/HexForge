import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => {
  const backend = {
    openFile: vi.fn(), closeFile: vi.fn(), getFileInfo: vi.fn(), readPage: vi.fn(), editByte: vi.fn(), undoEdit: vi.fn(), getDirtyState: vi.fn(),
    saveAs: vi.fn(), searchBytes: vi.fn(), applyTemplate: vi.fn(), loadTemplate: vi.fn(), saveTemplate: vi.fn(), saveTemplateAs: vi.fn(), unloadTemplateFile: vi.fn(), exportResultsCsv: vi.fn(),
  }
  return {
    backend, open: vi.fn(), save: vi.fn(), message: vi.fn(), confirm: vi.fn(),
    closeHandler: undefined as ((event: { preventDefault(): void }) => Promise<void>) | undefined,
    dropHandler: undefined as ((event: { payload: { type: string; paths: string[] } }) => void) | undefined,
    unlistenClose: vi.fn(), unlistenDrop: vi.fn(), windowClose: vi.fn(), windowSetFocus: vi.fn(), windowDestroyEditor: vi.fn(),
    windowOpen: vi.fn(),
    busSent: vi.fn(),
    busHandlers: new Map<string, (payload: unknown) => void | Promise<void>>(),
    lastDraft: null as unknown,
    failFlush: false,
  }
})

vi.mock('./api/backend', () => ({ backend: mocks.backend }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open, save: mocks.save, message: mocks.message, confirm: mocks.confirm }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({
  onCloseRequested: vi.fn(async (handler) => { mocks.closeHandler = handler; return mocks.unlistenClose }),
  close: mocks.windowClose,
  setFocus: mocks.windowSetFocus,
}) }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({
  onDragDropEvent: vi.fn(async (handler) => { mocks.dropHandler = handler; return mocks.unlistenDrop }),
}) }))
vi.mock('./templateWindow/windowManager', () => ({ templateWindowManager: { openOrFocus: mocks.windowOpen, forget: vi.fn(), close: vi.fn(), destroy: mocks.windowDestroyEditor } }))
vi.mock('./templateWindow/tauriBus', () => ({ tauriBus: {
  listen: vi.fn(async (event, callback) => { mocks.busHandlers.set(event, callback); return () => { mocks.busHandlers.delete(event) } }),
  send: vi.fn(async (_target, event, payload) => {
    mocks.busSent(event, payload)
    if (event === 'hexforge:template:flush') {
      if (mocks.failFlush) throw new Error('Template Editor did not respond.')
      const request = payload as { requestId: number }
      await mocks.busHandlers.get('hexforge:template:flush-reply')?.({ requestId: request.requestId, draft: mocks.lastDraft })
    }
  }),
} }))

import App from './App.vue'
import AppShell from './components/AppShell.vue'
import type { TemplateDefinition } from './types'

const file = { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false }
const page = { offset: '0', bytes: [0x41], modifiedOffsets: [], revision: '1' }

describe('App desktop orchestration', () => {
  beforeEach(() => {
    localStorage.clear()
    Object.values(mocks.backend).forEach((mock) => mock.mockReset())
    mocks.open.mockReset(); mocks.save.mockReset(); mocks.confirm.mockReset(); mocks.message.mockReset()
    mocks.unlistenClose.mockReset(); mocks.unlistenDrop.mockReset(); mocks.windowClose.mockReset(); mocks.windowClose.mockResolvedValue(undefined); mocks.windowSetFocus.mockReset(); mocks.windowSetFocus.mockResolvedValue(undefined); mocks.closeHandler = undefined; mocks.dropHandler = undefined
    mocks.windowOpen.mockReset(); mocks.windowOpen.mockResolvedValue(undefined); mocks.windowDestroyEditor.mockReset(); mocks.windowDestroyEditor.mockResolvedValue(undefined); mocks.busHandlers.clear(); mocks.busSent.mockReset(); mocks.lastDraft = null; mocks.failFlush = false
    mocks.backend.openFile.mockResolvedValue(file); mocks.backend.readPage.mockResolvedValue(page)
    mocks.backend.undoEdit.mockResolvedValue({ dirty: false, revision: '2', undone: true })
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: false, revision: '1' })
    mocks.backend.applyTemplate.mockResolvedValue([])
    mocks.backend.searchBytes.mockResolvedValue({ matches: [], truncated: false })
  })

  it('opens or focuses the one Template Editor window from its retained shortcut', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.windowOpen).toHaveBeenCalledTimes(1)
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.windowOpen).toHaveBeenCalledTimes(2)
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('marks template edits as modified and clears the marker after Save As succeeds', async () => {
    mocks.save.mockResolvedValue('C:/header.json')
    mocks.backend.saveTemplateAs.mockResolvedValue(undefined)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    expect(wrapper.findComponent(AppShell).props('menuState')!.templateActive).toBe(true)
    expect(press('s', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(mocks.backend.saveTemplate).not.toHaveBeenCalled()
    expect(press('s', { ctrlKey: true, shiftKey: true }).defaultPrevented).toBe(true); await flushPromises()
    expect(mocks.save).toHaveBeenCalledOnce()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
    expect(mocks.backend.saveTemplateAs).toHaveBeenCalledOnce()
    const event = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(event)
    expect(mocks.confirm).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('keeps an Untitled draft unsaved when Save As is cancelled', async () => {
    mocks.save.mockResolvedValue(null)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplateAs).not.toHaveBeenCalled()
    expect(wrapper.findComponent(AppShell).props('templateSource')).toBe('draft')
    const event = { preventDefault: vi.fn() }
    mocks.confirm.mockResolvedValue(false)
    await mocks.closeHandler?.(event)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('prompts before Save As replaces an existing template and binds the path only after success', async () => {
    mocks.save.mockResolvedValue('C:/existing.json')
    mocks.backend.saveTemplateAs.mockRejectedValueOnce({ code: 'destination_exists', message: 'The destination already exists.' })
      .mockRejectedValueOnce({ code: 'destination_exists', message: 'The destination already exists.' }).mockResolvedValue(undefined)
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.confirm).toHaveBeenCalledWith('File already exists. Overwrite?', expect.any(Object))
    expect(wrapper.findComponent(AppShell).props('templateSource')).toBe('draft')
    expect(mocks.backend.saveTemplateAs).toHaveBeenCalledTimes(1)
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplateAs).toHaveBeenLastCalledWith('C:/existing.json', expect.any(Object), true)
    expect(wrapper.findComponent(AppShell).props('templateDisplayName')).toBe('existing.json')
    expect(wrapper.findComponent(AppShell).props('menuState')!.templateHasPath).toBe(true)
    wrapper.unmount()
  })

  it('prompts before overwriting an externally changed template and leaves edits dirty on cancel', async () => {
    mocks.open.mockResolvedValue('C:/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Header', defaultEndianness: 'little', fields: [] })
    mocks.backend.saveTemplate.mockRejectedValueOnce({ code: 'external_modification', message: 'Changed.' })
      .mockRejectedValueOnce({ code: 'external_modification', message: 'Changed.' }).mockResolvedValue(undefined)
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('s', { ctrlKey: true }); await flushPromises()
    expect(mocks.confirm).toHaveBeenCalledWith('The file has been modified by another program. Overwrite?', expect.any(Object))
    expect(mocks.backend.saveTemplate).toHaveBeenCalledTimes(1)
    press('s', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplate).toHaveBeenLastCalledWith(expect.any(Object), true)
    expect(mocks.backend.applyTemplate).not.toHaveBeenCalled()
    expect(wrapper.findComponent(AppShell).props('templateDisplayName')).toBe('header.json')
    wrapper.unmount()
  })

  it('tells the Template Editor a cancelled Save As did not complete', async () => {
    mocks.save.mockResolvedValue(null)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    await sendEditorAction('save-as')
    const reply = mocks.busSent.mock.calls.filter(([event]) => event === 'hexforge:template:reply').at(-1)?.[1]
    expect(reply).toEqual(expect.objectContaining({ ok: true, completed: false }))
    expect(wrapper.findComponent(AppShell).props('templateSource')).toBe('draft')
    wrapper.unmount()
  })

  it('keeps edits made during Save newer than the saved editor checkpoint', async () => {
    mocks.open.mockResolvedValue('C:/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Header', defaultEndianness: 'little', fields: [] })
    let completeSave!: () => void
    mocks.backend.saveTemplate.mockImplementation(() => new Promise<void>((resolve) => { completeSave = resolve }))
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    await sendEditorDraft({ ...wrapper.findComponent(AppShell).props('template')!, name: 'Saved draft' }, true)
    const saving = sendEditorAction('save')
    await flushPromises()
    mocks.lastDraft = { sessionId: 'test-editor', sequence: 2, template: { ...wrapper.findComponent(AppShell).props('template')!, name: 'Later edit' }, valid: true }
    await mocks.busHandlers.get('hexforge:template:draft')?.(mocks.lastDraft)
    completeSave()
    await saving
    const latest = mocks.busSent.mock.calls.filter(([event]) => event === 'hexforge:template:snapshot').at(-1)?.[1]
    expect(latest).toEqual(expect.objectContaining({
      template: expect.objectContaining({ name: 'Later edit' }),
      checkpointTemplate: expect.objectContaining({ name: 'Saved draft' }),
      dirty: true,
    }))
    wrapper.unmount()
  })

  it('restores the pre-editor template on Don\'t Save and retains its earlier modified state', async () => {
    mocks.confirm.mockResolvedValue(false)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    await mocks.busHandlers.get('hexforge:template:closed')?.({ sessionId: 'test-editor' })
    const before = wrapper.findComponent(AppShell).props('template')!
    await sendEditorDraft({ ...before, name: 'Temporary edit' }, true, 1, 'second-editor')
    await sendEditorAction('discard')
    expect(wrapper.findComponent(AppShell).props('template')).toEqual(before)
    const event = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(event)
    expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining('template changes'), expect.any(Object))
    expect(event.preventDefault).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('keeps a dirty template when unload is cancelled and clears it after confirmation', async () => {
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    press('u', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    press('u', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(0)
    expect(mocks.confirm).toHaveBeenCalledTimes(2)
    wrapper.unmount()
  })

  it('unloads parsed results and the cyan range without closing the binary', async () => {
    mocks.open.mockResolvedValueOnce('C:/firmware.bin').mockResolvedValueOnce('C:/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Header', defaultEndianness: 'little', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] })
    mocks.backend.applyTemplate.mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    press('Enter', { ctrlKey: true }); await flushPromises()
    await wrapper.get('[data-testid="parsed-result"]').trigger('click'); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('templateRange')).toEqual({ start: 0n, end: 0n, count: 1n })
    press('u', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('file')?.name).toBe('firmware.bin')
    expect(wrapper.findComponent(AppShell).props('results')).toEqual([])
    expect(wrapper.findComponent(AppShell).props('templateRange')).toBeNull()
    expect(mocks.backend.closeFile).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('loads a template from the top menu without auto-opening the editor or binary file', async () => {
    mocks.open.mockResolvedValue('C:/specs/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Internal title', defaultEndianness: 'big', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('No template loaded.')
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(mocks.backend.loadTemplate).toHaveBeenCalledWith('C:/specs/header.json')
    expect(mocks.windowOpen).not.toHaveBeenCalled()
    expect(wrapper.findComponent(AppShell).props('template')!.name).toBe('Internal title')
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Template: "header.json" loaded. Open binary file to preview.')
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    expect(mocks.backend.applyTemplate).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('transitions from open binary without template to pending, applied, pending, and unloaded', async () => {
    mocks.open.mockResolvedValueOnce('C:/firmware.bin').mockResolvedValueOnce('C:/specs/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Internal title', defaultEndianness: 'little', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] })
    mocks.backend.applyTemplate.mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('No template loaded. Load or create template from Template menu.')
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Template: "header.json" loaded, pending apply.')
    expect(wrapper.findAll('.results-content button').map((button) => button.text())).toEqual(['Template Editor', 'Apply Template'])
    await wrapper.get('[data-action="results-apply-template"]').trigger('click'); await flushPromises()
    expect(wrapper.findAll('[data-testid="parsed-result"]')).toHaveLength(1)
    expect(wrapper.findAll('.results-content button')).toHaveLength(1)
    const template = wrapper.findComponent(AppShell).props('template')!
    await sendEditorDraft({ ...template, name: 'Edited title' }, true)
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Template: "header.json" loaded, pending apply.')
    expect(mocks.backend.applyTemplate).toHaveBeenCalledTimes(1)
    mocks.confirm.mockResolvedValue(true)
    press('u', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('No template loaded. Load or create template from Template menu.')
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
    wrapper.unmount()
  })

  it('applies an unsaved editor draft without saving it to JSON', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.backend.applyTemplate.mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendEditorDraft({ version: 1, name: 'My draft', defaultEndianness: 'little', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] }, true)
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Unsaved template draft: "My draft".')
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
    await mocks.busHandlers.get('hexforge:template:action')?.({ requestId: 1, command: 'apply', draft: mocks.lastDraft })
    await flushPromises()
    expect(mocks.backend.applyTemplate).toHaveBeenCalledOnce()
    expect(wrapper.findAll('[data-testid="parsed-result"]')).toHaveLength(1)
    expect(mocks.backend.saveTemplate).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('brings the main window forward for a template picker requested by the editor', async () => {
    mocks.open.mockResolvedValue('C:/header.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Loaded', defaultEndianness: 'little', fields: [] })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendEditorDraft({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }, true)
    await mocks.busHandlers.get('hexforge:template:action')?.({ requestId: 1, command: 'load', draft: mocks.lastDraft })
    await flushPromises()
    expect(mocks.windowSetFocus).toHaveBeenCalledOnce()
    expect(mocks.backend.loadTemplate).toHaveBeenCalledWith('C:/header.json')
    expect(mocks.windowOpen).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('preserves a dirty template when loading invalid JSON fails', async () => {
    mocks.open.mockResolvedValue('C:/bad.json')
    mocks.confirm.mockResolvedValue(true)
    mocks.backend.loadTemplate.mockRejectedValue({ code: 'invalid_template', message: 'Invalid template JSON.' })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    expect(wrapper.get('[role="dialog"]').text()).toContain('Invalid template JSON.')
    expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining('unsaved changes'), expect.any(Object))
    wrapper.unmount()
  })

  it('keeps the template draft dirty when Save Template As runs out of disk space', async () => {
    mocks.save.mockResolvedValue('C:/header.json')
    mocks.backend.saveTemplateAs.mockRejectedValue({ code: 'disk_full', message: 'Not enough disk space to save this template.' })
    mocks.confirm.mockResolvedValue(false)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Not enough disk space')
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    await wrapper.get('[aria-label="Close dialog"]').trigger('click'); await flushPromises()
    const event = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(event)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining('template changes'), expect.any(Object))
    wrapper.unmount()
  })

  it('blocks the main-window X when the editor cannot synchronize its draft', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendEditorDraft({ version: 1, name: 'Draft', defaultEndianness: 'little', fields: [] }, true)
    mocks.failFlush = true
    const event = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(event); await flushPromises()
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Template Editor did not respond.')
    wrapper.unmount()
  })

  it('shows a friendly error when the separate editor window cannot open', async () => {
    mocks.windowOpen.mockRejectedValue(new Error('Template Editor could not be opened.'))
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Template Editor could not be opened.')
    wrapper.unmount()
  })

  it('opens any selected binary path and renders the returned session', async () => {
    mocks.open.mockResolvedValue('C:/firmware.custom')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } })
    await flushPromises(); press('o', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.openFile).toHaveBeenCalledWith('C:/firmware.custom', false)
    expect(wrapper.text()).toContain('firmware.bin')
    wrapper.unmount()
  })

  it('routes exactly one dropped path through open and unregisters native listeners', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    mocks.dropHandler?.({ payload: { type: 'drop', paths: ['C:/drop.rom'] } }); await flushPromises()
    expect(mocks.backend.openFile).toHaveBeenCalledWith('C:/drop.rom', false)
    mocks.dropHandler?.({ payload: { type: 'drop', paths: ['a.bin', 'b.bin'] } }); await flushPromises()
    expect(mocks.backend.openFile).toHaveBeenCalledTimes(1)
    wrapper.unmount(); expect(mocks.unlistenClose).toHaveBeenCalledOnce(); expect(mocks.unlistenDrop).toHaveBeenCalledOnce()
  })

  it('treats native dialog cancellation as a silent no-op', async () => {
    mocks.open.mockResolvedValue(null)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.openFile).not.toHaveBeenCalled(); expect(mocks.message).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('presents rejected native open dialogs without an unhandled action failure', async () => {
    mocks.open.mockRejectedValue(new Error('Native open failed.'))
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Native open failed.')
    wrapper.unmount()
  })

  it('presents rejected native save dialogs without an unhandled action failure', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.save.mockRejectedValue(new Error('Native save failed.'))
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('s', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Native save failed.')
    expect(mocks.backend.saveAs).not.toHaveBeenCalled(); wrapper.unmount()
  })

  it('leaves the original close request unblocked for a clean session', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    const event = { preventDefault: vi.fn() }
    if (mocks.closeHandler) await mocks.closeHandler(event)
    expect(event.preventDefault).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('shuts down the Template Editor before allowing a clean main-window close', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendEditorDraft(wrapper.findComponent(AppShell).props('template')!, true)
    let finishEditorClose!: () => void
    mocks.windowDestroyEditor.mockImplementation(() => new Promise<void>((resolve) => { finishEditorClose = resolve }))
    const event = { preventDefault: vi.fn() }
    let mainCloseReady = false
    const closeAttempt = mocks.closeHandler?.(event).then(() => { mainCloseReady = true })
    await flushPromises()
    expect(mocks.windowDestroyEditor).toHaveBeenCalledOnce()
    expect(mainCloseReady).toBe(false)
    finishEditorClose()
    await closeAttempt
    expect(mainCloseReady).toBe(true)
    expect(event.preventDefault).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('warns before exit when only a template draft is unsaved', async () => {
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    const cancelled = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(cancelled)
    expect(cancelled.preventDefault).toHaveBeenCalledOnce()
    expect(mocks.windowDestroyEditor).not.toHaveBeenCalled()
    expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining('template'), expect.any(Object))
    const approved = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(approved)
    expect(approved.preventDefault).not.toHaveBeenCalled()
    expect(mocks.windowDestroyEditor).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('prevents a dirty close until discard is confirmed', async () => {
    mocks.open.mockResolvedValue('C:/dirty.bin')
    mocks.backend.openFile.mockResolvedValue({ ...file, dirty: true })
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: true, revision: '1' })
    mocks.confirm.mockResolvedValue(false)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    const event = { preventDefault: vi.fn() }
    if (mocks.closeHandler) await mocks.closeHandler(event)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    mocks.confirm.mockResolvedValue(true)
    const confirmedEvent = { preventDefault: vi.fn() }
    if (mocks.closeHandler) await mocks.closeHandler(confirmedEvent)
    expect(confirmedEvent.preventDefault).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('presents confirm and message failures from void native listener paths', async () => {
    mocks.open.mockResolvedValue('C:/dirty.bin'); mocks.backend.openFile.mockResolvedValue({ ...file, dirty: true })
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: true, revision: '1' })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    mocks.confirm.mockRejectedValueOnce(new Error('Confirm failed.'))
    if (mocks.closeHandler) await mocks.closeHandler({ preventDefault: vi.fn() }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Confirm failed.')
    await wrapper.get('[aria-label="Close dialog"]').trigger('click')
    mocks.message.mockRejectedValueOnce(new Error('Message failed.'))
    mocks.dropHandler?.({ payload: { type: 'drop', paths: ['a.bin', 'b.bin'] } }); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Message failed.')
    wrapper.unmount()
  })

  it('holds close until an in-flight edit is reflected by authoritative dirty state', async () => {
    let finishEdit!: (value: { dirty: boolean; revision: string }) => void
    const edit = new Promise<{ dirty: boolean; revision: string }>((resolve) => { finishEdit = resolve })
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.backend.editByte.mockReturnValue(edit)
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: true, revision: '2' })
    mocks.confirm.mockResolvedValue(false)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    wrapper.findComponent(AppShell).vm.$emit('select', { start: 0n, end: 0n, count: 1n })
    wrapper.findComponent(AppShell).vm.$emit('edit-request', 0n); await flushPromises()
    await wrapper.get('.prompt-form input').setValue('FF')
    void wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    const event = { preventDefault: vi.fn() }
    const closeAttempt = mocks.closeHandler?.(event)
    expect(event.preventDefault).not.toHaveBeenCalled(); expect(mocks.backend.getDirtyState).not.toHaveBeenCalled(); expect(mocks.confirm).not.toHaveBeenCalled()
    finishEdit({ dirty: true, revision: '2' }); await closeAttempt; await flushPromises()
    expect(mocks.confirm).toHaveBeenCalledOnce(); expect(event.preventDefault).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('closes without confirmation after a pending no-op edit resolves clean', async () => {
    let finishEdit!: (value: { dirty: boolean; revision: string }) => void
    const edit = new Promise<{ dirty: boolean; revision: string }>((resolve) => { finishEdit = resolve })
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.backend.editByte.mockReturnValue(edit)
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: false, revision: '1' })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    wrapper.findComponent(AppShell).vm.$emit('select', { start: 0n, end: 0n, count: 1n })
    wrapper.findComponent(AppShell).vm.$emit('edit-request', 0n); await flushPromises()
    await wrapper.get('.prompt-form input').setValue('41')
    void wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    const event = { preventDefault: vi.fn() }; const closeAttempt = mocks.closeHandler?.(event)
    expect(event.preventDefault).not.toHaveBeenCalled(); expect(mocks.backend.getDirtyState).not.toHaveBeenCalled()
    finishEdit({ dirty: false, revision: '1' }); await closeAttempt; await flushPromises()
    expect(event.preventDefault).not.toHaveBeenCalled(); expect(mocks.confirm).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('keeps close prevented and shows a friendly error when authoritative state fails', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.backend.getDirtyState.mockRejectedValue({ code: 'operation_failed', message: 'Could not check unsaved changes.' })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    const event = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(event); await flushPromises()
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Could not check unsaved changes.')
    wrapper.unmount()
  })

  it('gates Ctrl+S without a file path and top Apply while a template draft is invalid', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    await sendEditorDraft(wrapper.findComponent(AppShell).props('template')!, false)
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(wrapper.find('.dialog-backdrop').exists()).toBe(false)
    wrapper.unmount()
  })

  it('re-enables local Save, Ctrl+S, and Apply after removing the invalid field', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.save.mockResolvedValue('C:/template.json'); mocks.backend.saveTemplateAs.mockResolvedValue(undefined); mocks.backend.saveTemplate.mockResolvedValue(undefined)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    await sendEditorDraft(wrapper.findComponent(AppShell).props('template')!, false)
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    await sendEditorDraft({ ...wrapper.findComponent(AppShell).props('template')!, fields: [
      { name: 'fixed', offset: '0', type: 'u8', comment: '' },
    ] }, true, 2)
    expect(wrapper.get('[data-menu-command="save-template"]').attributes('disabled')).toBeDefined()
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.save).toHaveBeenCalledOnce(); expect(mocks.backend.saveTemplateAs).toHaveBeenCalledOnce()
    expect(wrapper.get('[data-menu-command="save-template"]').attributes('disabled')).toBeUndefined()
    press('s', { ctrlKey: true }); await flushPromises()
    expect(mocks.save).toHaveBeenCalledOnce(); expect(mocks.backend.saveTemplate).toHaveBeenCalledOnce()
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeUndefined()
    press('Enter', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.applyTemplate).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('wires template navigation to both selection and the cyan Canvas range', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    wrapper.findComponent(AppShell).vm.$emit('navigate', { start: 4n, end: 7n }); await flushPromises()
    const shell = wrapper.findComponent(AppShell)
    expect(shell.props('selection')).toEqual({ start: 4n, end: 7n, count: 4n })
    expect(shell.props('templateRange')).toEqual({ start: 4n, end: 7n, count: 4n })
    wrapper.unmount()
  })

  it('shows when parsed results need applying again after a template edit', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.backend.applyTemplate.mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('Enter', { ctrlKey: true }); await flushPromises()
    expect(wrapper.get('[data-testid="template-state"]').text()).toContain('Applied')
    const template = wrapper.findComponent(AppShell).props('template')!
    await sendEditorDraft({ ...template, name: 'Revised' }, true)
    expect(wrapper.get('[data-testid="template-state"]').text()).toContain('Unsaved draft')
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Unsaved template draft: "Revised".')
    wrapper.unmount()
  })

  it('clears a field highlight and its selection when the template draft changes', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    wrapper.findComponent(AppShell).vm.$emit('navigate', { start: 4n, end: 7n }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('templateRange')).not.toBeNull()
    const template = wrapper.findComponent(AppShell).props('template')!
    await sendEditorDraft({ ...template, name: 'Changed' }, true)
    expect(wrapper.findComponent(AppShell).props('templateRange')).toBeNull()
    expect(wrapper.findComponent(AppShell).props('selection')).toBeNull()
    wrapper.unmount()
  })

  it('clears the prior field highlight after a replacement template loads', async () => {
    mocks.open.mockResolvedValueOnce('C:/firmware.bin').mockResolvedValueOnce('C:/new.json')
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'New', defaultEndianness: 'little', fields: [] })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    wrapper.findComponent(AppShell).vm.$emit('navigate', { start: 4n, end: 7n }); await flushPromises()
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('templateRange')).toBeNull()
    expect(wrapper.findComponent(AppShell).props('selection')).toBeNull()
    wrapper.unmount()
  })

  it('normalizes a hex editor offset before applying the template through the backend boundary', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    const template = wrapper.findComponent(AppShell).props('template')!
    await sendEditorDraft({ ...template, fields: [{ ...template.fields[0]!, offset: '0x20' }] }, true)
    press('Enter', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.applyTemplate).toHaveBeenCalledWith(
      expect.objectContaining({ fields: [expect.objectContaining({ offset: '32' })] }), expect.any(Function),
    )
    wrapper.unmount()
  })

  it('shows live search activity and reports a truncated backend result', async () => {
    let resolveSearch!: (value: { matches: string[]; truncated: boolean }) => void
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.backend.searchBytes.mockImplementation(() => new Promise((resolve) => { resolveSearch = resolve }))
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('f', { ctrlKey: true }); await flushPromises()
    await wrapper.get('.prompt-form input').setValue('41 42')
    await wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('Searching bytes')
    resolveSearch({ matches: ['0'], truncated: true }); await flushPromises()
    expect(wrapper.find('[data-testid="status-progress"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="search-truncated"]').text()).toContain('limited')
    wrapper.unmount()
  })

  it('routes File shortcuts to Open, Save As, Export, Close and Exit', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    mocks.save.mockResolvedValueOnce('C:/results.csv').mockResolvedValueOnce('C:/copy.bin')
    mocks.backend.saveAs.mockResolvedValue({ dirty: false, revision: '0', bytesWritten: '32', destination: 'C:/copy.bin',
      file: { name: 'copy.bin', path: 'C:/copy.bin', size: '32', revision: '0', dirty: false } })
    mocks.backend.applyTemplate.mockResolvedValue([{ name: 'x', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    mocks.backend.exportResultsCsv.mockResolvedValue(undefined); mocks.backend.closeFile.mockResolvedValue(undefined)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    press('Enter', { ctrlKey: true }); await flushPromises()
    press('e', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.backend.exportResultsCsv).toHaveBeenCalledWith('C:/results.csv', expect.any(Object), expect.any(Function))
    press('s', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(mocks.backend.saveAs).toHaveBeenCalledWith('C:/copy.bin', expect.any(Function))
    press('w', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.closeFile).toHaveBeenCalledWith(false)
    expect(wrapper.find('[data-testid="drop-prompt"]').exists()).toBe(true)
    press('F4', { altKey: true }); await flushPromises()
    expect(mocks.windowClose).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('confirms before Close File discards in-memory edits', async () => {
    mocks.open.mockResolvedValue('C:/dirty.bin'); mocks.backend.openFile.mockResolvedValue({ ...file, dirty: true }); mocks.backend.closeFile.mockResolvedValue(undefined)
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('w', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.closeFile).not.toHaveBeenCalled()
    press('w', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.closeFile).toHaveBeenCalledWith(true)
    wrapper.unmount()
  })

  it('routes Edit shortcuts through edit mode, F2 and the undo stack', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.backend.editByte.mockResolvedValue({ dirty: true, revision: '2' })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    wrapper.findComponent(AppShell).vm.$emit('select', { start: 0n, end: 0n, count: 1n }); await flushPromises()
    press('e', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('editMode')).toBe(true)
    press('F2'); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Edit byte')
    await wrapper.get('.prompt-form input').setValue('FF'); await wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    press('z', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.editByte).toHaveBeenCalledWith(0n, 0xff)
    expect(mocks.backend.undoEdit).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('routes Navigate shortcuts to the go-to and byte-search dialogs', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.backend.searchBytes.mockResolvedValue({ matches: ['8'], truncated: false })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('g', { ctrlKey: true }); await flushPromises()
    await wrapper.get('.prompt-form input').setValue('0x4'); await wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('selection')).toEqual({ start: 4n, end: 4n, count: 1n })
    press('f', { ctrlKey: true }); await flushPromises()
    await wrapper.get('.prompt-form input').setValue('41 42'); await wrapper.get('.prompt-form').trigger('submit'); await flushPromises()
    expect(mocks.backend.searchBytes).toHaveBeenCalledWith('41 42', expect.any(Function))
    wrapper.unmount()
  })

  it('routes the retained Template shortcuts without a main-window Add Field shortcut', async () => {
    mocks.open.mockResolvedValueOnce('C:/firmware.bin').mockResolvedValueOnce('C:/header.json')
    mocks.save.mockResolvedValue('C:/header-copy.json'); mocks.backend.saveTemplateAs.mockResolvedValue(undefined); mocks.backend.saveTemplate.mockResolvedValue(undefined)
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Loaded', defaultEndianness: 'big', fields: [] })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.windowOpen).toHaveBeenCalledOnce()
    await sendDraftWithField(wrapper.findComponent(AppShell).props('template')!)
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    expect(press('a', { ctrlKey: true, altKey: true }).defaultPrevented).toBe(false)
    expect(wrapper.findComponent(AppShell).props('template')!.fields).toHaveLength(1)
    press('Enter', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.applyTemplate).toHaveBeenCalledOnce()
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplateAs).toHaveBeenCalledWith('C:/header-copy.json', expect.any(Object), false)
    press('s', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplate).toHaveBeenCalledWith(expect.any(Object), false)
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(mocks.backend.loadTemplate).toHaveBeenCalledWith('C:/header.json')
    expect(mocks.windowOpen).toHaveBeenCalledOnce()
    press('u', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('templateSource')).toBe('none')
    wrapper.unmount()
  })

  it('routes the simplified View shortcuts without binding direct themes or panel visibility', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    expect(document.documentElement.dataset.theme).toBe('dark')
    press('t', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(document.documentElement.dataset.theme).toBe('light')
    const directThemeEvents = [press('d', { ctrlKey: true, altKey: true }), press('l', { ctrlKey: true, altKey: true })]
    await flushPromises()
    expect(directThemeEvents.every((event) => !event.defaultPrevented)).toBe(true)
    expect(document.documentElement.dataset.theme).toBe('light')
    press('2', { ctrlKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('bytesPerRow')).toBe(32)
    press('1', { ctrlKey: true }); await flushPromises()
    expect(wrapper.findComponent(AppShell).props('bytesPerRow')).toBe(16)
    const panelEvents = [press('b', { ctrlKey: true }), press('b', { ctrlKey: true, altKey: true })]
    await flushPromises()
    expect(panelEvents.every((event) => !event.defaultPrevented)).toBe(true)
    expect(wrapper.get('[data-testid="hexforge-app"]').classes()).not.toContain('results-collapsed')
    wrapper.unmount()
  })
})

function press(key: string, options: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...options })
  window.dispatchEvent(event)
  return event
}

async function sendEditorDraft(template: TemplateDefinition, valid: boolean, sequence = 1, sessionId = 'test-editor'): Promise<void> {
  await mocks.busHandlers.get('hexforge:template:ready')?.({ sessionId })
  mocks.lastDraft = { sessionId, sequence, template, valid }
  await mocks.busHandlers.get('hexforge:template:draft')?.(mocks.lastDraft)
  await flushPromises()
}

async function sendEditorAction(command: string): Promise<void> {
  await mocks.busHandlers.get('hexforge:template:action')?.({ requestId: 42, command, draft: mocks.lastDraft })
  await flushPromises()
}

async function sendDraftWithField(template: TemplateDefinition): Promise<void> {
  await sendEditorDraft({ ...template, fields: [...template.fields,
    { name: 'field1', offset: '0', type: 'u8', endianness: template.defaultEndianness, comment: '' },
  ] }, true)
}
