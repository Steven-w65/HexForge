import { describe, expect, it, vi } from 'vitest'
import { createMainBridge } from './mainBridge'
import { createEditorBridge } from './editorBridge'
import { events, type LocalBus, type TemplateSnapshot } from './protocol'
import type { TemplateDefinition } from '../types'

const empty: TemplateDefinition = { name: 'Untitled', defaultEndianness: 'little', fields: [] }

function bus(): LocalBus {
  const handlers = new Map<string, Set<(payload: unknown) => void>>()
  return {
    async listen(event, callback) { const set = handlers.get(event) ?? new Set(); set.add(callback); handlers.set(event, set); return () => { set.delete(callback) } },
    async send(_target, event, payload) { for (const callback of handlers.get(event) ?? []) await callback(payload) },
  }
}

function snapshot(): Omit<TemplateSnapshot, 'revision' | 'ackSequence'> {
  return { template: empty, checkpointTemplate: empty, results: [], fileSize: null, theme: 'dark', dirty: false, canApply: false, active: false, templateFilePath: null, persistenceRevision: 0, workspaceRevision: 0 }
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

  it('reports a cancelled Save As action so the editor stays open', async () => {
    const transport = bus(); const onReady = vi.fn()
    const main = createMainBridge(transport, { snapshot, onDraft: vi.fn(), onAction: vi.fn().mockResolvedValue(false), onReady })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    expect(onReady).toHaveBeenCalledOnce()
    await expect(editor.requestAction('save-as')).resolves.toBe(false)
    main.dispose(); editor.dispose()
  })

  it('waits beyond five seconds while a native Save As picker is still open', async () => {
    vi.useFakeTimers()
    const transport = bus()
    let finish!: () => void
    const picker = new Promise<void>((resolve) => { finish = resolve })
    const main = createMainBridge(transport, { snapshot, onDraft: vi.fn(), onAction: () => picker })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    try {
      await main.start(); await editor.start()
      const outcome = editor.requestAction('save-as').then((value) => value, (error) => error)
      await vi.advanceTimersByTimeAsync(6000)
      finish()
      expect(await outcome).toBe(true)
    } finally {
      main.dispose(); editor.dispose(); vi.useRealTimers()
    }
  })

  it('replaces an unacknowledged old draft on a workspace replacement and ignores late snapshots and acknowledgements', async () => {
    const underlying = bus()
    const transport: LocalBus = {
      listen: underlying.listen,
      send: (target, event, payload) => event === events.draft ? Promise.resolve() : underlying.send(target, event, payload),
    }
    let state = snapshot()
    const main = createMainBridge(transport, { snapshot: () => state, onDraft: vi.fn(), onAction: vi.fn() })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    const fresh: TemplateSnapshot['template'] = { name: 'Untitled', defaultEndianness: 'little', fields: [] }
    await main.start(); await editor.start()
    await editor.sendDraft({ ...empty, name: 'Old unsent draft' }, true)
    state = { ...state, workspaceRevision: 1, persistenceRevision: 1, template: fresh, checkpointTemplate: fresh }
    await main.publish()
    expect(editor.state()?.template).toEqual(fresh)

    await underlying.send('template-editor', events.snapshot, { ...snapshot(), revision: 999, ackSequence: 100, workspaceRevision: 0 })
    await underlying.send('template-editor', events.ack, { sessionId: editor.draft().sessionId, sequence: 100, workspaceRevision: 0 })
    await editor.sendDraft({ ...fresh, name: 'New workspace edit' }, true)
    expect(editor.state()?.template.name).toBe('New workspace edit')
    expect(editor.isAcknowledged()).toBe(false)
    main.dispose(); editor.dispose()
  })

  it('does not accept late draft or action messages from a discarded workspace', async () => {
    const transport = bus()
    const onDraft = vi.fn()
    const onAction = vi.fn()
    const fresh: TemplateSnapshot['template'] = { name: 'Untitled', defaultEndianness: 'little', fields: [] }
    let state = { ...snapshot(), workspaceRevision: 1, template: fresh }
    const main = createMainBridge(transport, { snapshot: () => state, onDraft: (draft) => { onDraft(draft); state = { ...state, template: draft.template } }, onAction })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    const oldDraft = { sessionId: editor.draft().sessionId, sequence: 1, workspaceRevision: 0, template: { ...empty, name: 'Discarded' }, valid: true }
    await transport.send('main', events.draft, oldDraft)
    await transport.send('main', events.action, { requestId: 77, command: 'save', draft: oldDraft })
    expect(onDraft).not.toHaveBeenCalled()
    expect(onAction).not.toHaveBeenCalled()
    expect(state.template).toEqual(fresh)
    main.dispose(); editor.dispose()
  })

  it('completes a confirmed discard even though restoring the draft replaces the workspace', async () => {
    const transport = bus()
    let state = { ...snapshot(), template: { ...empty, name: 'Unsaved edit' }, checkpointTemplate: empty, dirty: true }
    const main = createMainBridge(transport, {
      snapshot: () => state,
      onDraft: vi.fn(),
      onAction: async (action) => {
        if (action.command === 'discard') state = { ...state, template: empty, dirty: false, workspaceRevision: state.workspaceRevision + 1 }
      },
    })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()

    expect(await editor.requestAction('discard')).toBe(true)
    expect(editor.state()?.template).toEqual(empty)
    main.dispose(); editor.dispose()
  })

  it('does not complete an old action from a late reply after the workspace changed', async () => {
    const underlying = bus()
    const transport: LocalBus = {
      listen: underlying.listen,
      send: (target, event, payload) => event === events.action ? Promise.resolve() : underlying.send(target, event, payload),
    }
    let state = snapshot()
    const main = createMainBridge(transport, { snapshot: () => state, onDraft: vi.fn(), onAction: vi.fn() })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    const oldSave = editor.requestAction('save')
    const fresh: TemplateSnapshot['template'] = { name: 'Untitled', defaultEndianness: 'little', fields: [] }
    state = { ...state, workspaceRevision: 1, template: fresh }
    await main.publish()
    await underlying.send('template-editor', events.reply, {
      requestId: 1, ok: true, completed: true,
      snapshot: { ...snapshot(), revision: 999, ackSequence: 100, workspaceRevision: 0 },
    })
    expect(await oldSave).toBe(false)
    expect(editor.state()?.template).toEqual(fresh)
    main.dispose(); editor.dispose()
  })

  it('keeps a late discard reply from closing an editor that has moved to another workspace', async () => {
    const underlying = bus()
    const transport: LocalBus = {
      listen: underlying.listen,
      send: (target, event, payload) => event === events.action ? Promise.resolve() : underlying.send(target, event, payload),
    }
    let state = snapshot()
    const main = createMainBridge(transport, { snapshot: () => state, onDraft: vi.fn(), onAction: vi.fn() })
    const editor = createEditorBridge(transport, { onSnapshot: vi.fn(), onError: vi.fn() })
    await main.start(); await editor.start()
    const pendingDiscard = editor.requestAction('discard')
    state = { ...state, workspaceRevision: 1, template: { ...empty, name: 'New workspace' } }
    await main.publish()
    await underlying.send('template-editor', events.reply, {
      requestId: 1, ok: true, completed: true,
      snapshot: { ...snapshot(), revision: 999, ackSequence: 100, workspaceRevision: 0 },
    })
    expect(await pendingDiscard).toBe(false)
    expect(editor.state()?.template.name).toBe('New workspace')
    main.dispose(); editor.dispose()
  })
})
