import { EDITOR_LABEL, MAIN_LABEL, events, type DraftUpdate, type EditorActionName, type EditorReply, type LocalBus, type TemplateSnapshot } from './protocol'

interface EditorCallbacks { onSnapshot(snapshot: TemplateSnapshot): void; onError(error: unknown): void }

export function createEditorBridge(bus: LocalBus, callbacks: EditorCallbacks) {
  const sessionId = `${Date.now()}-${Math.random().toString(36).slice(2)}`
  const disposers: Array<() => void> = []
  const waiting = new Map<number, { resolve: () => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> }>()
  let current: TemplateSnapshot | null = null
  let valid = true
  let sequence = 0
  let acknowledged = 0
  let lastRevision = 0
  let requestId = 0

  function applySnapshot(incoming: TemplateSnapshot): void {
    if (!incoming || incoming.revision < lastRevision) return
    lastRevision = incoming.revision
    acknowledged = Math.max(acknowledged, incoming.ackSequence)
    // Keep locally typed text until the main window confirms that exact sequence.
    current = incoming.ackSequence < sequence && current ? { ...incoming, template: current.template } : incoming
    callbacks.onSnapshot(current)
  }

  function draft(): DraftUpdate {
    return { sessionId, sequence, template: current?.template ?? { version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }, valid }
  }

  async function start(): Promise<void> {
    try {
      disposers.push(await bus.listen(events.snapshot, (payload) => { applySnapshot(payload as TemplateSnapshot) }))
      disposers.push(await bus.listen(events.ack, (payload) => {
        const ack = payload as { sessionId: string; sequence: number }
        if (ack.sessionId === sessionId) acknowledged = Math.max(acknowledged, ack.sequence)
      }))
      disposers.push(await bus.listen(events.reply, (payload) => {
        const reply = payload as EditorReply
        const pending = waiting.get(reply?.requestId)
        if (!pending) return
        clearTimeout(pending.timer); waiting.delete(reply.requestId)
        if (reply.snapshot) applySnapshot(reply.snapshot)
        if (reply.ok) pending.resolve(); else pending.reject(new Error(reply.error ?? 'Template action failed.'))
      }))
      disposers.push(await bus.listen(events.flush, async (payload) => {
        const request = payload as { requestId: number }
        try { await bus.send(MAIN_LABEL, events.flushReply, { requestId: request.requestId, draft: draft() }) }
        catch (error) { callbacks.onError(error) }
      }))
      await bus.send(MAIN_LABEL, events.ready, { sessionId })
    } catch (error) { dispose(); throw error }
  }

  async function sendDraft(template: DraftUpdate['template'], isValid: boolean): Promise<void> {
    sequence += 1; valid = isValid
    if (current) current = { ...current, template, dirty: true }
    try { await bus.send(MAIN_LABEL, events.draft, draft()) }
    catch (error) { callbacks.onError(error); throw error }
  }

  async function requestAction(command: EditorActionName, range?: { start: string; end: string }): Promise<void> {
    const id = ++requestId
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => { waiting.delete(id); reject(new Error('Template action timed out. Your draft is preserved.')) }, 5000)
      waiting.set(id, { resolve, reject, timer })
      void bus.send(MAIN_LABEL, events.action, { requestId: id, command, draft: draft(), range }).catch((error) => {
        clearTimeout(timer); waiting.delete(id); reject(error)
      })
    })
  }

  function dispose(): void {
    disposers.splice(0).forEach((dispose) => dispose())
    for (const pending of waiting.values()) { clearTimeout(pending.timer); pending.reject(new Error('Template Editor closed.')) }
    waiting.clear()
    void bus.send(MAIN_LABEL, events.closed, { sessionId }).catch(() => undefined)
  }

  async function notifyClosed(): Promise<void> { await bus.send(MAIN_LABEL, events.closed, { sessionId }) }

  return { start, state: () => current, sendDraft, requestAction, notifyClosed, dispose, draft, isAcknowledged: () => acknowledged >= sequence }
}
