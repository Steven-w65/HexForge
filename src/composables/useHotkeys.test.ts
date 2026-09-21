import { afterEach, describe, expect, it, vi } from 'vitest'
import { handleCloseRequest, useHotkeys, type HotkeyActions } from './useHotkeys'

function actions(popupOpen = false): HotkeyActions {
  return {
    open: vi.fn(), search: vi.fn(), goTo: vi.fn(), saveTemplate: vi.fn(), undo: vi.fn(),
    isPopupOpen: () => popupOpen, closePopup: vi.fn(), clearSelection: vi.fn(),
  }
}

function keydown(key: string, options: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...options })
  window.dispatchEvent(event)
  return event
}

describe('useHotkeys', () => {
  const cleanups: Array<() => void> = []
  afterEach(() => { cleanups.splice(0).forEach((cleanup) => cleanup()) })

  it('dispatches specified shortcuts and lets Escape close a popup before clearing selection', () => {
    const target = actions(true); cleanups.push(useHotkeys(target))
    keydown('o', { ctrlKey: true }); keydown('f', { ctrlKey: true }); keydown('Escape')
    expect(target.open).toHaveBeenCalledOnce(); expect(target.search).toHaveBeenCalledOnce()
    expect(target.closePopup).toHaveBeenCalledOnce(); expect(target.clearSelection).not.toHaveBeenCalled()
  })

  it('routes go-to, template save and undo while preventing default browser actions', () => {
    const target = actions(); cleanups.push(useHotkeys(target))
    expect(keydown('g', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(keydown('s', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(keydown('z', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(target.goTo).toHaveBeenCalledOnce(); expect(target.saveTemplate).toHaveBeenCalledOnce(); expect(target.undo).toHaveBeenCalledOnce()
  })

  it('does not hijack text editing shortcuts and removes its listener on cleanup', () => {
    const target = actions(); const dispose = useHotkeys(target)
    const input = document.createElement('input'); document.body.append(input)
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true, cancelable: true }))
    expect(target.undo).not.toHaveBeenCalled()
    dispose(); keydown('o', { ctrlKey: true }); expect(target.open).not.toHaveBeenCalled()
    input.remove()
  })

  it.each([
    ['o', 'open'], ['f', 'search'], ['g', 'goTo'], ['s', 'saveTemplate'],
  ] as const)('dispatches Ctrl+%s from an input to %s', (key, action) => {
    const target = actions(); cleanups.push(useHotkeys(target))
    const input = document.createElement('input'); document.body.append(input)
    const event = new KeyboardEvent('keydown', { key, ctrlKey: true, bubbles: true, cancelable: true })
    input.dispatchEvent(event)
    expect(target[action]).toHaveBeenCalledOnce(); expect(event.defaultPrevented).toBe(true)
    input.remove()
  })

  it('requires exact Ctrl shortcuts and ignores shifted variants', () => {
    const target = actions(); cleanups.push(useHotkeys(target))
    for (const key of ['o', 'f', 'g', 's', 'z']) keydown(key, { ctrlKey: true, shiftKey: true })
    expect(target.open).not.toHaveBeenCalled(); expect(target.search).not.toHaveBeenCalled(); expect(target.goTo).not.toHaveBeenCalled()
    expect(target.saveTemplate).not.toHaveBeenCalled(); expect(target.undo).not.toHaveBeenCalled()
  })

  it('clears selection with Escape when no popup is open', () => {
    const target = actions(); cleanups.push(useHotkeys(target)); keydown('Escape')
    expect(target.clearSelection).toHaveBeenCalledOnce()
  })

  it('prevents close until dirty discard is confirmed and allows exactly one retry', async () => {
    const event = { preventDefault: vi.fn() }; const close = vi.fn(); const guard = { value: false }
    await handleCloseRequest(event, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(false), close, guard)
    expect(event.preventDefault).toHaveBeenCalledOnce(); expect(close).not.toHaveBeenCalled()
    await handleCloseRequest(event, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(true), close, guard)
    expect(close).toHaveBeenCalledOnce(); expect(guard.value).toBe(true)
    const retry = { preventDefault: vi.fn() }
    await handleCloseRequest(retry, vi.fn(), vi.fn(), close, guard)
    expect(retry.preventDefault).not.toHaveBeenCalled(); expect(guard.value).toBe(false)
  })

  it('serializes close confirmations and rolls back allowClose when close fails', async () => {
    const decision = deferredBoolean(); const confirm = vi.fn(() => decision.promise); const close = vi.fn().mockRejectedValue(new Error('close failed'))
    const guard = { value: false }; const first = { preventDefault: vi.fn() }; const second = { preventDefault: vi.fn() }
    const query = vi.fn().mockResolvedValue({ dirty: true })
    const a = handleCloseRequest(first, query, confirm, close, guard).catch(() => undefined)
    const b = handleCloseRequest(second, query, confirm, close, guard).catch(() => undefined)
    await Promise.resolve()
    expect(confirm).toHaveBeenCalledOnce(); expect(second.preventDefault).toHaveBeenCalledOnce()
    decision.resolve(true); await Promise.all([a, b])
    expect(close).toHaveBeenCalledOnce(); expect(guard.value).toBe(false)
  })

  it('prevents first, waits for authoritative state, and closes a clean session through one retry', async () => {
    const state = deferredDirty(); const event = { preventDefault: vi.fn() }; const close = vi.fn(); const guard = { value: false }
    const pending = handleCloseRequest(event, vi.fn(() => state.promise), vi.fn(), close, guard)
    expect(event.preventDefault).toHaveBeenCalledOnce(); expect(close).not.toHaveBeenCalled()
    state.resolve({ dirty: false }); await pending
    expect(close).toHaveBeenCalledOnce(); expect(guard.value).toBe(true)
  })

  it('observes an edit that becomes dirty while the close query waits', async () => {
    const state = deferredDirty(); const confirm = vi.fn().mockResolvedValue(false); const close = vi.fn()
    const pending = handleCloseRequest({ preventDefault: vi.fn() }, () => state.promise, confirm, close, { value: false })
    state.resolve({ dirty: true }); await pending
    expect(confirm).toHaveBeenCalledOnce(); expect(close).not.toHaveBeenCalled()
  })

  it('keeps the window prevented and surfaces authoritative query failures', async () => {
    const event = { preventDefault: vi.fn() }; const failure = new Error('state unavailable'); const release = vi.fn()
    await expect(handleCloseRequest(event, vi.fn().mockRejectedValue(failure), vi.fn(), vi.fn(), { value: false }, release)).rejects.toBe(failure)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(release).toHaveBeenCalledOnce()
  })

  it('releases the mutation barrier when discard is declined or native close fails', async () => {
    const release = vi.fn()
    await handleCloseRequest({ preventDefault: vi.fn() }, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(false), vi.fn(), { value: false }, release)
    expect(release).toHaveBeenCalledOnce()
    await expect(handleCloseRequest({ preventDefault: vi.fn() }, vi.fn().mockResolvedValue({ dirty: false }), vi.fn(), vi.fn().mockRejectedValue(new Error('close failed')), { value: false }, release)).rejects.toThrow('close failed')
    expect(release).toHaveBeenCalledTimes(2)
  })
})

function deferredBoolean() {
  let resolve!: (value: boolean) => void
  const promise = new Promise<boolean>((done) => { resolve = done })
  return { promise, resolve }
}

function deferredDirty() {
  let resolve!: (value: { dirty: boolean }) => void
  const promise = new Promise<{ dirty: boolean }>((done) => { resolve = done })
  return { promise, resolve }
}
