import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { TemplateSnapshot } from '../templateWindow/protocol'

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (payload: unknown) => void | Promise<void>>(),
  sent: vi.fn(), destroy: vi.fn(), closeHandler: null as null | ((event: { preventDefault(): void }) => void),
  failAction: false,
}))

const initial: TemplateSnapshot = {
  revision: 1, ackSequence: 0, template: { version: 1, name: 'Header', defaultEndianness: 'little', fields: [] },
  results: [], fileSize: '32', theme: 'light', dirty: false, canApply: true, active: true,
}

vi.mock('../templateWindow/tauriBus', () => ({ tauriBus: {
  listen: vi.fn(async (event, callback) => { mocks.handlers.set(event, callback); return () => { mocks.handlers.delete(event) } }),
  send: vi.fn(async (target, event, payload) => {
    mocks.sent(target, event, payload)
    if (event === 'hexforge:template:ready') await mocks.handlers.get('hexforge:template:snapshot')?.(initial)
    if (event === 'hexforge:template:action') {
      const action = payload as { requestId: number }
      await mocks.handlers.get('hexforge:template:reply')?.({ requestId: action.requestId, ok: !mocks.failAction, error: mocks.failAction ? 'Template file is missing.' : undefined })
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
    mocks.closeHandler = null; mocks.failAction = false
    initial.canApply = true; initial.active = true
  })

  it('renders the existing field editor with context and sends edits and actions to main', async () => {
    const wrapper = mount(TemplateEditorWindow); await flushPromises()
    expect(wrapper.get('[data-testid="template-editor-window"]').text()).toContain('Header')
    expect(document.documentElement.dataset.theme).toBe('light')
    await wrapper.get('[data-action="add-field"]').trigger('click'); await flushPromises()
    expect(wrapper.findAll('.field-card')).toHaveLength(1)
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:draft', expect.objectContaining({ template: expect.objectContaining({ fields: [expect.any(Object)] }) }))
    await wrapper.get('[data-action="save-template"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'save' }))
    await wrapper.get('[data-action="navigate-field"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'navigate', range: { start: '0', end: '0' } }))
    await wrapper.get('[data-action="apply-template"]').trigger('click'); await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'apply' }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'u', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'unload' }))
    wrapper.unmount()
  })

  it('keeps a draft and shows an error after a failed action, then flushes on X', async () => {
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
    expect(mocks.sent).toHaveBeenCalledWith('main', 'hexforge:template:action', expect.objectContaining({ command: 'close', draft: expect.objectContaining({ template: expect.objectContaining({ fields: [expect.any(Object)] }) }) }))
    expect(mocks.destroy).toHaveBeenCalledOnce()
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
})
