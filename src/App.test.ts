import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => {
  const backend = {
    openFile: vi.fn(), closeFile: vi.fn(), getFileInfo: vi.fn(), readPage: vi.fn(), editByte: vi.fn(), undoEdit: vi.fn(), getDirtyState: vi.fn(),
    saveAs: vi.fn(), searchBytes: vi.fn(), applyTemplate: vi.fn(), loadTemplate: vi.fn(), saveTemplate: vi.fn(), exportResultsCsv: vi.fn(),
  }
  return {
    backend, open: vi.fn(), save: vi.fn(), message: vi.fn(), confirm: vi.fn(),
    closeHandler: undefined as ((event: { preventDefault(): void }) => Promise<void>) | undefined,
    dropHandler: undefined as ((event: { payload: { type: string; paths: string[] } }) => void) | undefined,
    unlistenClose: vi.fn(), unlistenDrop: vi.fn(), windowClose: vi.fn(),
  }
})

vi.mock('./api/backend', () => ({ backend: mocks.backend }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open, save: mocks.save, message: mocks.message, confirm: mocks.confirm }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({
  onCloseRequested: vi.fn(async (handler) => { mocks.closeHandler = handler; return mocks.unlistenClose }),
  close: mocks.windowClose,
}) }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({
  onDragDropEvent: vi.fn(async (handler) => { mocks.dropHandler = handler; return mocks.unlistenDrop }),
}) }))

import App from './App.vue'
import AppShell from './components/AppShell.vue'

const file = { name: 'firmware.bin', path: 'C:/firmware.bin', size: '32', revision: '1', dirty: false }
const page = { offset: '0', bytes: [0x41], modifiedOffsets: [], revision: '1' }

describe('App desktop orchestration', () => {
  beforeEach(() => {
    localStorage.clear()
    Object.values(mocks.backend).forEach((mock) => mock.mockReset())
    mocks.open.mockReset(); mocks.save.mockReset(); mocks.confirm.mockReset(); mocks.message.mockReset()
    mocks.unlistenClose.mockReset(); mocks.unlistenDrop.mockReset(); mocks.windowClose.mockReset(); mocks.windowClose.mockResolvedValue(undefined); mocks.closeHandler = undefined; mocks.dropHandler = undefined
    mocks.backend.openFile.mockResolvedValue(file); mocks.backend.readPage.mockResolvedValue(page)
    mocks.backend.undoEdit.mockResolvedValue({ dirty: false, revision: '2', undone: true })
    mocks.backend.getDirtyState.mockResolvedValue({ dirty: false, revision: '1' })
    mocks.backend.applyTemplate.mockResolvedValue([])
    mocks.backend.searchBytes.mockResolvedValue({ matches: [], truncated: false })
  })

  it('toggles the Template Editor from its retained keyboard shortcut', async () => {
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(true)
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('marks template edits as modified and clears the marker after Save As succeeds', async () => {
    mocks.save.mockResolvedValue('C:/header.json')
    mocks.backend.saveTemplate.mockResolvedValue(undefined)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    expect(wrapper.get('[data-testid="template-modified"]').text()).toContain('Modified')
    await wrapper.get('[data-action="save-template"]').trigger('click'); await flushPromises()
    expect(mocks.backend.saveTemplate).toHaveBeenCalledOnce()
    expect(wrapper.find('[data-testid="template-modified"]').exists()).toBe(false)
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
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
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

  it('warns before exit when only a template draft is unsaved', async () => {
    mocks.confirm.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    await wrapper.get('[data-action="close-template-editor"]').trigger('click')
    const cancelled = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(cancelled)
    expect(cancelled.preventDefault).toHaveBeenCalledOnce()
    expect(mocks.confirm).toHaveBeenCalledWith(expect.stringContaining('template'), expect.any(Object))
    const approved = { preventDefault: vi.fn() }
    await mocks.closeHandler?.(approved)
    expect(approved.preventDefault).not.toHaveBeenCalled()
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

  it('gates Ctrl+S and top Apply while a template draft is invalid', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    await wrapper.get('input[data-field="offset"]').setValue('0x'); await flushPromises()
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(wrapper.find('.dialog-backdrop').exists()).toBe(false)
    wrapper.unmount()
  })

  it('re-enables local Save, Ctrl+S, and Apply after removing the invalid field', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin'); mocks.save.mockResolvedValue('C:/template.json'); mocks.backend.saveTemplate.mockResolvedValue(undefined)
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    await wrapper.get('input[data-field="offset"]').setValue('0x'); await flushPromises()
    await wrapper.get('[title="Remove field"]').trigger('click'); await flushPromises()
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="apply-template"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeUndefined()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(mocks.save).toHaveBeenCalledOnce(); expect(mocks.backend.saveTemplate).toHaveBeenCalledOnce()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    await wrapper.get('[data-menu="template"]').trigger('click')
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

  it('normalizes a hex editor offset before applying the template through the backend boundary', async () => {
    mocks.open.mockResolvedValue('C:/firmware.bin')
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    await wrapper.get('input[data-field="offset"]').setValue('0x20'); await flushPromises()
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
    press('a', { ctrlKey: true, altKey: true }); await flushPromises()
    press('Enter', { ctrlKey: true }); await flushPromises()
    press('e', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(mocks.backend.exportResultsCsv).toHaveBeenCalledWith('C:/results.csv', expect.any(Object), expect.any(Function))
    press('s', { ctrlKey: true, shiftKey: true }); await flushPromises()
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

  it('routes Template shortcuts to add, apply, load and Save Template As', async () => {
    mocks.open.mockResolvedValueOnce('C:/firmware.bin').mockResolvedValueOnce('C:/header.json')
    mocks.save.mockResolvedValue('C:/header-copy.json'); mocks.backend.saveTemplate.mockResolvedValue(undefined)
    mocks.backend.loadTemplate.mockResolvedValue({ version: 1, name: 'Loaded', defaultEndianness: 'big', fields: [] })
    const wrapper = mount(App, { global: { stubs: { HexCanvas: true } } }); await flushPromises()
    press('o', { ctrlKey: true }); await flushPromises()
    press('t', { ctrlKey: true, shiftKey: true }); await flushPromises()
    expect(wrapper.find('[data-testid="template-editor-panel"]').exists()).toBe(true)
    press('a', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findAll('.field-card')).toHaveLength(1)
    await wrapper.get('select[data-field="type"]').setValue('u32'); await flushPromises()
    press('a', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(wrapper.findAll<HTMLInputElement>('input[data-field="offset"]')[1]!.element.value).toBe('4')
    press('Enter', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.applyTemplate).toHaveBeenCalledOnce()
    press('s', { ctrlKey: true }); await flushPromises()
    expect(mocks.backend.saveTemplate).toHaveBeenCalledWith('C:/header-copy.json', expect.any(Object))
    press('o', { ctrlKey: true, altKey: true }); await flushPromises()
    expect(mocks.backend.loadTemplate).toHaveBeenCalledWith('C:/header.json')
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
