import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { TemplateSnapshot } from '../templateWindow/protocol'

enableAutoUnmount(afterEach)

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (payload: unknown) => void | Promise<void>>(),
  sent: vi.fn(), destroy: vi.fn(), closeHandler: null as null | ((event: { preventDefault(): void }) => void),
  failAction: false, cancelAction: false, discardReplacesWorkspace: false, saveSnapshot: null as TemplateSnapshot | null,
  holdActionReply: null as Promise<void> | null,
}))

const initial: TemplateSnapshot = {
  revision: 1, ackSequence: 0, template: { name: 'Header', defaultEndianness: 'little', fields: [] },
  checkpointTemplate: { name: 'Header', defaultEndianness: 'little', fields: [] },
  results: [], fileSize: '32', theme: 'light', dirty: false, canApply: true, active: true,
  templateFilePath: 'C:/header.json', persistenceRevision: 1, workspaceRevision: 0,
}

vi.mock('../templateWindow/tauriBus', () => ({ tauriBus: {
  listen: vi.fn(async (event, callback) => { mocks.handlers.set(event, callback); return () => { mocks.handlers.delete(event) } }),
  send: vi.fn(async (target, event, payload) => {
    mocks.sent(target, event, payload)
    if (event === 'hexforge:template:ready') await mocks.handlers.get('hexforge:template:snapshot')?.(initial)
    if (event === 'hexforge:template:action') {
      await mocks.holdActionReply
      const action = payload as { requestId: number; command: string }
      const restored = (action.command === 'save' || action.command === 'save-as') && mocks.saveSnapshot ? mocks.saveSnapshot
        : action.command === 'discard' && mocks.discardReplacesWorkspace
        ? { ...initial, revision: initial.revision + 1, workspaceRevision: initial.workspaceRevision + 1,
          template: initial.checkpointTemplate, dirty: false }
        : undefined
      await mocks.handlers.get('hexforge:template:reply')?.({ requestId: action.requestId, ok: !mocks.failAction, completed: !mocks.cancelAction,
        error: mocks.failAction ? 'Template file is missing.' : undefined, snapshot: restored })
    }
  }),
} }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({
  onCloseRequested: vi.fn(async (handler) => { mocks.closeHandler = handler; return () => undefined }), destroy: mocks.destroy,
}) }))

import TemplateEditorWindow from './TemplateEditorWindow.vue'

describe('TemplateEditorWindow', () => {
  beforeEach(() => {
    mocks.handlers.clear(); mocks.sent.mockReset(); mocks.destroy.mockReset(); mocks.destroy.mockResolvedValue(undefined)
    mocks.closeHandler = null; mocks.failAction = false; mocks.cancelAction = false; mocks.discardReplacesWorkspace = false
    mocks.saveSnapshot = null
    mocks.holdActionReply = null
    initial.canApply = true; initial.active = true; initial.templateFilePath = 'C:/header.json'; initial.dirty = false; initial.persistenceRevision = 1; initial.workspaceRevision = 0
    initial.template = { name: 'Header', defaultEndianness: 'little', fields: [] }
    initial.checkpointTemplate = { ...initial.template }
    initial.results = []
  })

  it('uses only the native title-bar close control', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    const hasInPageCloseButton = wrapper.find('.editor-header button[aria-label="Close"], .editor-header button[data-action="close"]').exists()
    const event = { preventDefault: vi.fn() }
    mocks.closeHandler?.(event); await flushPromises()
    wrapper.unmount()
    expect(hasInPageCloseButton).toBe(false)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(mocks.destroy).toHaveBeenCalledOnce()
  })

  it('closes a clean editor with Ctrl+W through its acknowledged close workflow', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    const event = new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })
    window.dispatchEvent(event); await flushPromises()
    expect(event.defaultPrevented).toBe(true)
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(false)
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'close' }))
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:closed', expect.any(Object))
    expect(mocks.destroy).toHaveBeenCalledOnce()
  })

  it('lets Ctrl+W Cancel preserve the draft and Don\'t Save close after discard acknowledgement', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field="template-name"]').setValue('Edited'); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    expect(mocks.destroy).not.toHaveBeenCalled()
    await wrapper.get('[data-action="editor-cancel"]').trigger('click'); await flushPromises()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(false)
    expect((wrapper.get('[data-field="template-name"]').element as HTMLInputElement).value).toBe('Edited')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    mocks.discardReplacesWorkspace = true
    await wrapper.get('[data-action="editor-discard"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'discard' }))
    expect(mocks.destroy).toHaveBeenCalledOnce()
  })

  it.each(['save', 'save-as'] as const)('allows a Ctrl+W close only after %s saves the current draft', async (choice) => {
    if (choice === 'save-as') initial.templateFilePath = null
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field="template-name"]').setValue('Edited'); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(mocks.destroy).not.toHaveBeenCalled()
    const saved = { name: 'Edited', defaultEndianness: 'little' as const, fields: [] }
    mocks.saveSnapshot = { ...initial, revision: 2, ackSequence: 10, persistenceRevision: 2,
      checkpointTemplate: saved, template: saved, templateFilePath: 'C:/saved.json', dirty: false }
    await wrapper.get(`[data-action="editor-${choice}"]`).trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: choice }))
    expect(mocks.destroy).toHaveBeenCalledOnce()
  })

  it('keeps Ctrl+W from closing through a field-removal confirmation', async () => {
    initial.template = { ...initial.template, fields: [{ name: 'configured', type: 'u8' }] }
    initial.checkpointTemplate = initial.template
    const wrapper = mount(TemplateEditorWindow, { attachTo: document.body }); await flushPromises()
    await wrapper.get('[data-action="remove-field"]').trigger('click'); await flushPromises()
    const event = new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })
    window.dispatchEvent(event); await flushPromises()
    expect(event.defaultPrevented).toBe(true)
    expect(wrapper.get('[role="dialog"]').text()).toContain('Remove field')
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(false)
    expect(mocks.destroy).not.toHaveBeenCalled()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
  })

  it('does not duplicate a close when Ctrl+W repeats before the first acknowledgement', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, repeat: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event, payload]) => event === 'hexforge:template:action' && payload.command === 'close')).toHaveLength(1)
    expect(mocks.destroy).toHaveBeenCalledOnce()
  })

  it.each([{ altKey: true }, { shiftKey: true }, { metaKey: true }])('does not close for Ctrl+W with extra modifiers %s', async (extra) => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    const event = new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true, ...extra })
    window.dispatchEvent(event); await flushPromises()
    expect(event.defaultPrevented).toBe(false)
    expect(mocks.destroy).not.toHaveBeenCalled()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(false)
  })

  it.each([false, true])('keeps Ctrl+S (shift=%s) bound to template persistence inside editor inputs', async (shiftKey) => {
    const wrapper = mount(TemplateEditorWindow, { attachTo: document.body }); await flushPromises()
    const input = wrapper.get('[data-field="template-name"]').element as HTMLInputElement
    input.focus()
    const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey, bubbles: true, cancelable: true })
    input.dispatchEvent(event); await flushPromises()
    expect(event.defaultPrevented).toBe(true)
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: shiftKey ? 'save-as' : 'save' }))
    expect(mocks.destroy).not.toHaveBeenCalled()
  })

  it('consumes repeated Save and Save As keys while the first editor save awaits acknowledgement', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    let finish!: () => void
    mocks.holdActionReply = new Promise<void>(resolve => { finish = resolve })
    try {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true }))
      await flushPromises()
      for (const shiftKey of [false, true]) {
        const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey, bubbles: true, cancelable: true })
        window.dispatchEvent(event); await flushPromises()
        expect(event.defaultPrevented).toBe(true)
      }
      expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(1)
    } finally { finish(); await flushPromises(); wrapper.unmount() }
  })

  it.each(['s', 'w'])('does not save or close the editor with composing Ctrl+%s', async (key) => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key, ctrlKey: true, isComposing: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    expect(mocks.destroy).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('suppresses the browser context menu inside the Template Editor', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    wrapper.get('input').element.dispatchEvent(event)
    wrapper.unmount()
    expect(event.defaultPrevented).toBe(true)
  })

  it('renders the existing field editor with context and sends edits and actions to main', async () => {
    initial.results = [{ kind: 'leaf', name: 'field1', path: 'field1', type: 'u8', offset: '0', length: '1', value: '41',
      endianness: 'little', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] }]
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    expect(wrapper.get('[data-testid="template-editor-window"]').text()).toContain('Header')
    expect(document.documentElement.dataset.theme).toBe('light')
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    expect(wrapper.findAll('.field-card')).toHaveLength(1)
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:draft', expect.objectContaining({ template: expect.objectContaining({ fields: [expect.any(Object)] }) }))
    await wrapper.get('[data-action="save-template"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'save' }))
    await wrapper.get('[data-action="save-template-as"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'save-as' }))
    await wrapper.get('[data-action="navigate-field"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'navigate', range: { start: '0', end: '0' } }))
    await wrapper.get('[data-action="apply-template"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'apply' }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'u', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'unload' }))
    wrapper.unmount()
  })

  it('keeps the separate window and bridge while editing a structured template', async () => {
    initial.template = { name: 'Records', defaultEndianness: 'little', fields: [{ name: 'count', type: 'u8' }] }
    initial.checkpointTemplate = initial.template
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    expect(wrapper.find('[data-action="add-field"]').exists()).toBe(true)
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:draft', expect.objectContaining({ template: expect.objectContaining({ fields: expect.any(Array) }) }))
    wrapper.unmount()
  })

  it('accepts authoritative theme updates without replacing an unacknowledged editor draft', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field="template-name"]').setValue('Unsaved title'); await flushPromises()
    await mocks.handlers.get('hexforge:template:snapshot')?.({ ...initial, revision: 2, theme: 'dark' })
    await flushPromises()
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect((wrapper.get('[data-field="template-name"]').element as HTMLInputElement).value).toBe('Unsaved title')
    await mocks.handlers.get('hexforge:template:snapshot')?.({ ...initial, revision: 3, theme: 'light' })
    await flushPromises()
    expect(document.documentElement.dataset.theme).toBe('light')
    expect((wrapper.get('[data-field="template-name"]').element as HTMLInputElement).value).toBe('Unsaved title')
    wrapper.unmount()
  })

  it('shows one editor without a format badge or selector', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    expect(wrapper.findAll('.editor-header [role="group"]')).toHaveLength(0)
    expect(wrapper.findAll('.editor-header button')).toHaveLength(0)
    expect(wrapper.get('.editor-header').text()).toContain('Header')
    wrapper.unmount()
  })

  it('keeps a draft and shows an error after a failed action, then asks before closing', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    mocks.failAction = true
    await wrapper.get('[data-action="load-template"]').trigger('click'); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Template file is missing.')
    expect(wrapper.findAll('.field-card')).toHaveLength(1)
    mocks.failAction = false
    const event = { preventDefault: vi.fn() }
    mocks.closeHandler?.(event); await flushPromises()
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(wrapper.get('[data-testid="editor-close-prompt"]').text()).toContain('Save As')
    expect(mocks.destroy).not.toHaveBeenCalled()
    mocks.discardReplacesWorkspace = true
    await wrapper.get('[data-action="editor-discard"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'discard' }))
    expect(mocks.destroy).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('disables Save without a path but routes Ctrl+Shift+S to Save As', async () => {
    initial.templateFilePath = null
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-action="save-template-as"]').attributes('disabled')).toBeUndefined()
    const saveEvent = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true })
    window.dispatchEvent(saveEvent); await flushPromises()
    expect(saveEvent.defaultPrevented).toBe(true)
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'save-as' }))
    wrapper.unmount()
  })

  it('uses validation for Save shortcuts and close-prompt buttons, not only the footer', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field="template-name"]').setValue(''); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    expect(wrapper.get('[data-action="editor-save"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-action="editor-save-as"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-action="editor-save"]').attributes('title')).toContain('Correct')
    wrapper.unmount()
  })

  it('disables conflicting editor actions when the authoritative main window is busy', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await mocks.handlers.get('hexforge:template:snapshot')?.({ ...initial, revision: 2, busy: true })
    await flushPromises()
    for (const name of ['load-template', 'unload-template', 'save-template', 'save-template-as']) {
      expect(wrapper.get(`[data-action="${name}"]`).attributes('disabled'), name).toBeDefined()
    }
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'o', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    await mocks.handlers.get('hexforge:template:snapshot')?.({ ...initial, revision: 3, busy: false })
    await flushPromises()
    expect(wrapper.get('[data-action="load-template"]').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('protects a rejected name edit on close and keeps both Save shortcuts disabled', async () => {
    initial.template = { ...initial.template, fields: [{ name: 'first', type: 'u8' }, { name: 'second', type: 'u8' }] }
    initial.checkpointTemplate = initial.template
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field-path="first"] [data-field="name"]').setValue('second'); await flushPromises()
    expect(wrapper.get('[data-testid="validation-summary"]').text()).toContain('already exists')
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:draft', expect.objectContaining({ valid: false, template: initial.template }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    expect(mocks.destroy).not.toHaveBeenCalled()
    await wrapper.get('[data-action="editor-cancel"]').trigger('click'); await flushPromises()
    expect((wrapper.get('[data-field-path="first"] [data-field="name"]').element as HTMLInputElement).value).toBe('second')
    await wrapper.get('[data-field-path="first"] [data-field="name"]').setValue('renamed'); await flushPromises()
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeUndefined()
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    await wrapper.get('[data-action="editor-discard"]').trigger('click'); await flushPromises()
    expect(mocks.destroy).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('does not save or load through a field-removal confirmation', async () => {
    initial.template = { ...initial.template, fields: [{ name: 'configured', type: 'u8' }] }
    initial.checkpointTemplate = initial.template
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-action="remove-field"]').trigger('click'); await flushPromises()
    expect(wrapper.get('[role="dialog"]').text()).toContain('Remove field')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'o', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    wrapper.unmount()
  })

  it('clears rejected UI inputs when a load replaces the workspace with identical JSON', async () => {
    initial.template = { ...initial.template, fields: [{ name: 'first', type: 'u8' }, { name: 'second', type: 'u8' }] }
    initial.checkpointTemplate = initial.template
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field-path="first"] [data-field="name"]').setValue('second'); await flushPromises()
    await mocks.handlers.get('hexforge:template:snapshot')?.({ ...initial, revision: 2, workspaceRevision: 1, persistenceRevision: 2, ackSequence: 10 })
    await flushPromises()
    expect(wrapper.find('[data-testid="validation-summary"]').exists()).toBe(false)
    expect((wrapper.get('[data-field-path="first"] [data-field="name"]').element as HTMLInputElement).value).toBe('first')
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('keeps the editor open if Save As is cancelled during the Ctrl+W close prompt', async () => {
    initial.templateFilePath = null
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    expect(wrapper.get('[data-action="editor-save"]').attributes('disabled')).toBeDefined()
    mocks.cancelAction = true
    await wrapper.get('[data-action="editor-save-as"]').trigger('click'); await flushPromises()
    expect(mocks.destroy).not.toHaveBeenCalled()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('retains the draft and Ctrl+W close prompt after a failed Save', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-field="template-name"]').setValue('Edited'); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    mocks.failAction = true
    await wrapper.get('[data-action="editor-save"]').trigger('click'); await flushPromises()
    expect(mocks.destroy).not.toHaveBeenCalled()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    expect((wrapper.get('[data-field="template-name"]').element as HTMLInputElement).value).toBe('Edited')
    expect(wrapper.text()).toContain('Template file is missing.')
    const actionsBefore = mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action').length
    const event = new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })
    window.dispatchEvent(event); await flushPromises()
    expect(event.defaultPrevented).toBe(true)
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(actionsBefore)
  })

  it('does not close after Ctrl+W Save if its reply contains a newer unsaved edit', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, bubbles: true, cancelable: true })); await flushPromises()
    const saved = { ...initial.template, fields: [{ name: 'field1', type: 'u8' as const }] }
    mocks.saveSnapshot = { ...initial, revision: 2, ackSequence: 10, persistenceRevision: 2,
      checkpointTemplate: saved, template: { ...saved, name: 'Newer unsaved edit' }, dirty: true }
    await wrapper.get('[data-action="editor-save"]').trigger('click'); await flushPromises()
    expect(mocks.destroy).not.toHaveBeenCalled()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    expect((wrapper.get('[data-field="template-name"]').element as HTMLInputElement).value).toBe('Newer unsaved edit')
    wrapper.unmount()
  })

  it('closes after a successful close-prompt Save acknowledges the current draft', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    const saved = { ...initial.template, fields: [{ name: 'field1', type: 'u8' as const }] }
    mocks.saveSnapshot = { ...initial, revision: 2, ackSequence: 10, persistenceRevision: 2,
      checkpointTemplate: saved, template: saved, dirty: false }
    await wrapper.get('[data-action="editor-save"]').trigger('click'); await flushPromises()
    expect(mocks.destroy).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('closes a view-only editor without changing an already modified template', async () => {
    initial.dirty = true
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(false)
    expect(mocks.destroy).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('prompts on close when a newer edit remains after a Save completes', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    await mocks.handlers.get('hexforge:template:snapshot')?.({
      ...initial, revision: 2, persistenceRevision: 2,
      template: { ...initial.template, name: 'Later edit' },
      checkpointTemplate: { ...initial.template, name: 'Saved draft' },
      dirty: true,
    })
    await flushPromises()
    mocks.closeHandler?.({ preventDefault: vi.fn() }); await flushPromises()
    expect(wrapper.find('[data-testid="editor-close-prompt"]').exists()).toBe(true)
    expect(mocks.destroy).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('does not invoke disabled Apply or Unload keyboard actions', async () => {
    initial.canApply = false; initial.active = false
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true, cancelable: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'u', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent.mock.calls.filter(([, event]) => event === 'hexforge:template:action')).toHaveLength(0)
    wrapper.unmount()
  })

  it('adds fields through the editor button but not the removed Add Field shortcut', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(wrapper.findAll('.field-card')).toHaveLength(0)
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    expect(wrapper.findAll('.field-card')).toHaveLength(1)
    wrapper.unmount()
  })
})
