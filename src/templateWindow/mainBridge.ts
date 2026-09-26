import { EDITOR_LABEL, events, errorMessage, type DraftUpdate, type EditorAction, type EditorReply, type LocalBus, type TemplateSnapshot } from './protocol'

interface MainCallbacks {
  snapshot(): Omit<TemplateSnapshot, 'revision' | 'ackSequence'>
  onDraft(draft: DraftUpdate): void | Promise<void>
  onAction(action: EditorAction): Promise<boolean | void>
  onReady?(): void
  onError?(error: unknown): void
  onClosed?(): void
}

export function createMainBridge(bus: LocalBus, callbacks: MainCallbacks) {
  const disposers: Array<() => void> = []
  const waiting = new Map<number, { resolve: () => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> }>()
  let sessionId: string | null = null
  let lastSequence = 0
  let revision = 0
  let requestId = 0
  let started = false

  function snapshot(): TemplateSnapshot { return { ...callbacks.snapshot(), revision: ++revision, ackSequence: lastSequence } }

  async function publish(): Promise<void> {
    if (!sessionId) return
    await bus.send(EDITOR_LABEL, events.snapshot, snapshot())
  }

  async function accept(draft: DraftUpdate): Promise<void> {
    if (draft.sessionId !== sessionId || draft.sequence <= lastSequence) return
    await callbacks.onDraft(draft)
    lastSequence = draft.sequence
    await bus.send(EDITOR_LABEL, events.ack, { sessionId, sequence: lastSequence })
  }

  function requireCurrentDraft(draft: DraftUpdate): void {
    if (!sessionId || draft?.sessionId !== sessionId) throw new Error('Template Editor is disconnected. Its draft is still open.')
  }

  async function start(): Promise<void> {
    if (started) return
    started = true
    try {
      disposers.push(await bus.listen(events.ready, async (payload) => {
        const id = (payload as { sessionId?: unknown })?.sessionId
        if (typeof id !== 'string') return
        sessionId = id; lastSequence = 0; revision = 0
        callbacks.onReady?.()
        try { await publish() } catch (error) { callbacks.onError?.(error) }
      }))
      disposers.push(await bus.listen(events.draft, async (payload) => {
        try { await accept(payload as DraftUpdate); await publish() } catch (error) { callbacks.onError?.(error) }
      }))
      disposers.push(await bus.listen(events.action, async (payload) => {
        const action = payload as EditorAction
        if (!action || typeof action.requestId !== 'number') return
        let reply: EditorReply = { requestId: action.requestId, ok: true }
        try {
          requireCurrentDraft(action.draft)
          await accept(action.draft)
          reply.completed = await callbacks.onAction(action) !== false
          reply.snapshot = snapshot()
        } catch (error) { reply = { requestId: action.requestId, ok: false, error: errorMessage(error) } }
        try { await bus.send(EDITOR_LABEL, events.reply, reply); await publish() } catch (error) { callbacks.onError?.(error) }
      }))
      disposers.push(await bus.listen(events.flushReply, async (payload) => {
        const response = payload as { requestId: number; draft: DraftUpdate }
        const pending = waiting.get(response?.requestId)
        if (!pending) return
        try { requireCurrentDraft(response.draft); await accept(response.draft); clearTimeout(pending.timer); waiting.delete(response.requestId); pending.resolve() }
        catch (error) { clearTimeout(pending.timer); waiting.delete(response.requestId); pending.reject(new Error(errorMessage(error))) }
      }))
      disposers.push(await bus.listen(events.closed, (payload) => {
        if ((payload as { sessionId?: string })?.sessionId === sessionId) { sessionId = null; callbacks.onClosed?.() }
      }))
    } catch (error) { dispose(); throw error }
  }

  async function flush(): Promise<void> {
    if (!sessionId) return
    const id = ++requestId
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => { waiting.delete(id); reject(new Error('Template Editor did not respond. Its draft is still open.')) }, 5000)
      waiting.set(id, { resolve, reject, timer })
      void bus.send(EDITOR_LABEL, events.flush, { requestId: id }).catch((error) => {
        clearTimeout(timer); waiting.delete(id); reject(new Error(errorMessage(error)))
      })
    })
  }

  function dispose(): void {
    disposers.splice(0).forEach((dispose) => dispose())
    for (const pending of waiting.values()) { clearTimeout(pending.timer); pending.reject(new Error('Template Editor disconnected.')) }
    waiting.clear(); sessionId = null; started = false
  }

  return { start, publish, flush, dispose, isReady: () => sessionId !== null }
}
