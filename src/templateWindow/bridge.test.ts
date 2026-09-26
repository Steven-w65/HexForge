import { describe, expect, it, vi } from 'vitest'
import { createMainBridge } from './mainBridge'
import { createEditorBridge } from './editorBridge'
import type { LocalBus, TemplateSnapshot } from './protocol'
import type { TemplateDefinition } from '../types'

const empty: TemplateDefinition = { version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }

function bus(): LocalBus {
  const handlers = new Map<string, Set<(payload: unknown) => void>>()
  return {
    async listen(event, callback) { const set = handlers.get(event) ?? new Set(); set.add(callback); handlers.set(event, set); return () => { set.delete(callback) } },
    async send(_target, event, payload) { for (const callback of handlers.get(event) ?? []) await callback(payload) },
  }
}

function snapshot(): Omit<TemplateSnapshot, 'revision' | 'ackSequence'> {
  return { template: empty, results: [], fileSize: null, theme: 'dark', dirty: false, canApply: false, active: false }
}

describe('local template-window bridge', () => {
  it('publishes after ready and acknowledges the latest field edit', async () => {
    const transport = bus(); const accepted = vi.fn(); let state = snapshot()
    const main = createMainBridge(transport, { snapshot: () => state, onDraft: (draft) => { accepted(draft); state = { ...state, template: draft.template } }, onAction: vi.fn() })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    expect(editor.state()?.template.name).toBe('Untitled')
    await editor.sendDraft({ ...empty, name: 'Header' }, true)
    await main.flush()
    expect(accepted).toHaveBeenCalledWith(expect.objectContaining({ template: expect.objectContaining({ name: 'Header' }) }))
    expect(editor.state()?.template.name).toBe('Header')
    main.dispose(); editor.dispose()
  })

  it('does not replace an unacknowledged draft with an older snapshot', async () => {
    const underlying = bus(); const transport: LocalBus = {
      listen: underlying.listen,
      send: (target, event, payload) => event === 'hexforge:template:draft' ? Promise.resolve() : underlying.send(target, event, payload),
    }
    let accepted = snapshot(); const main = createMainBridge(transport, {
      snapshot: () => accepted, onDraft: () => undefined, onAction: vi.fn(),
    })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    await editor.sendDraft({ ...empty, name: 'Pending' }, true)
    accepted = { ...accepted, template: empty }
    await main.publish()
    expect(editor.state()?.template.name).toBe('Pending')
    main.dispose(); editor.dispose()
  })

  it('retains the local draft when its event send fails', async () => {
    const underlying = bus(); const onError = vi.fn()
    const transport: LocalBus = {
      listen: underlying.listen,
      send: (target, event, payload) => event === 'hexforge:template:draft' ? Promise.reject(new Error('Event unavailable.')) : underlying.send(target, event, payload),
    }
    const main = createMainBridge(transport, { snapshot, onDraft: vi.fn(), onAction: vi.fn() })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError })
    await main.start(); await editor.start()
    await expect(editor.sendDraft({ ...empty, name: 'Preserved' }, true)).rejects.toThrow('Event unavailable.')
    await main.publish()
    expect(editor.state()?.template.name).toBe('Preserved')
    expect(onError).toHaveBeenCalledOnce()
    main.dispose(); editor.dispose()
  })
})
