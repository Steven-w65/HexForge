import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MenuCommand } from '../menu/commands'
import { handleCloseRequest, useHotkeys, type HotkeyActions } from './useHotkeys'

function actions(popupOpen = false): HotkeyActions {
  return {
    invoke: vi.fn(), isEnabled: vi.fn(() => true),
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

  it('dispatches shortcuts through the shared command layer and lets Escape close a popup first', () => {
    const target = actions(true); cleanups.push(useHotkeys(target))
    keydown('o', { ctrlKey: true }); keydown('f', { ctrlKey: true }); keydown('Escape')
    expect(target.invoke).toHaveBeenNthCalledWith(1, 'open'); expect(target.invoke).toHaveBeenNthCalledWith(2, 'search')
    expect(target.closePopup).toHaveBeenCalledOnce(); expect(target.clearSelection).not.toHaveBeenCalled()
  })

  it.each([
    ['o', { ctrlKey: true }, 'open'], ['w', { ctrlKey: true }, 'close-file'],
    ['s', { ctrlKey: true, shiftKey: true }, 'save-as'], ['e', { ctrlKey: true, shiftKey: true }, 'export'],
    ['F4', { altKey: true }, 'exit'], ['F2', {}, 'edit-selected'],
    ['e', { ctrlKey: true, altKey: true }, 'toggle-edit'], ['z', { ctrlKey: true }, 'undo'],
    ['g', { ctrlKey: true }, 'goto'], ['f', { ctrlKey: true }, 'search'],
    ['Enter', { ctrlKey: true }, 'apply-template'], ['o', { ctrlKey: true, altKey: true }, 'load-template'],
    ['s', { ctrlKey: true }, 'save-template'], ['a', { ctrlKey: true, altKey: true }, 'add-field'],
    ['t', { ctrlKey: true, altKey: true }, 'theme-toggle'], ['1', { ctrlKey: true }, 'row-16'],
    ['2', { ctrlKey: true }, 'row-32'],
  ] satisfies Array<[string, KeyboardEventInit, MenuCommand]>)('routes %s to %s and prevents the browser default', (key, options, command) => {
    const target = actions(); cleanups.push(useHotkeys(target))
    expect(keydown(key, options).defaultPrevented).toBe(true)
    expect(target.invoke).toHaveBeenCalledWith(command)
  })

  it('does not hijack text editing Undo or F2 and removes its listener on cleanup', () => {
    const target = actions(); const dispose = useHotkeys(target)
    const input = document.createElement('input'); document.body.append(input)
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true, cancelable: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true, cancelable: true }))
    expect(target.invoke).not.toHaveBeenCalled()
    dispose(); keydown('o', { ctrlKey: true }); expect(target.invoke).not.toHaveBeenCalled()
    input.remove()
  })

  it('does not invoke or consume a disabled command', () => {
    const target = actions(); vi.mocked(target.isEnabled).mockReturnValue(false); cleanups.push(useHotkeys(target))
    const event = keydown('g', { ctrlKey: true })
    expect(target.invoke).not.toHaveBeenCalled(); expect(event.defaultPrevented).toBe(false)
  })

  it('does not consume removed theme-selection or panel-visibility shortcuts', () => {
    const target = actions(); cleanups.push(useHotkeys(target))
    const events = [
      keydown('d', { ctrlKey: true, altKey: true }),
      keydown('l', { ctrlKey: true, altKey: true }),
      keydown('b', { ctrlKey: true }),
      keydown('b', { ctrlKey: true, altKey: true }),
    ]
    expect(events.every((event) => !event.defaultPrevented)).toBe(true)
    expect(target.invoke).not.toHaveBeenCalled()
  })

  it('clears selection with Escape when no popup is open', () => {
    const target = actions(); cleanups.push(useHotkeys(target)); keydown('Escape')
    expect(target.clearSelection).toHaveBeenCalledOnce()
  })

  it('prevents a dirty close only when discard is declined', async () => {
    const guard = { confirming: false }
    const declined = { preventDefault: vi.fn() }
    await handleCloseRequest(declined, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(false), guard)
    expect(declined.preventDefault).toHaveBeenCalledOnce()

    const confirmed = { preventDefault: vi.fn() }
    await handleCloseRequest(confirmed, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(true), guard)
    expect(confirmed.preventDefault).not.toHaveBeenCalled()
  })

  it('serializes close confirmations and prevents overlapping close requests', async () => {
    const decision = deferredBoolean(); const confirm = vi.fn(() => decision.promise)
    const guard = { confirming: false }; const first = { preventDefault: vi.fn() }; const second = { preventDefault: vi.fn() }
    const query = vi.fn().mockResolvedValue({ dirty: true })
    const a = handleCloseRequest(first, query, confirm, guard)
    const b = handleCloseRequest(second, query, confirm, guard)
    await Promise.resolve()
    expect(confirm).toHaveBeenCalledOnce(); expect(second.preventDefault).toHaveBeenCalledOnce()
    decision.resolve(true); await Promise.all([a, b])
    expect(first.preventDefault).not.toHaveBeenCalled()
  })

  it('waits for authoritative state and leaves a clean close request unblocked', async () => {
    const state = deferredDirty(); const event = { preventDefault: vi.fn() }; const guard = { confirming: false }
    const pending = handleCloseRequest(event, vi.fn(() => state.promise), vi.fn(), guard)
    expect(event.preventDefault).not.toHaveBeenCalled()
    state.resolve({ dirty: false }); await pending
    expect(event.preventDefault).not.toHaveBeenCalled()
  })

  it('observes an edit that becomes dirty while the close query waits', async () => {
    const state = deferredDirty(); const confirm = vi.fn().mockResolvedValue(false)
    const event = { preventDefault: vi.fn() }
    const pending = handleCloseRequest(event, () => state.promise, confirm, { confirming: false })
    state.resolve({ dirty: true }); await pending
    expect(confirm).toHaveBeenCalledOnce(); expect(event.preventDefault).toHaveBeenCalledOnce()
  })

  it('keeps the window prevented and surfaces authoritative query failures', async () => {
    const event = { preventDefault: vi.fn() }; const failure = new Error('state unavailable'); const release = vi.fn()
    await expect(handleCloseRequest(event, vi.fn().mockRejectedValue(failure), vi.fn(), { confirming: false }, release)).rejects.toBe(failure)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(release).toHaveBeenCalledOnce()
  })

  it('releases the mutation barrier when discard is declined', async () => {
    const release = vi.fn()
    await handleCloseRequest({ preventDefault: vi.fn() }, vi.fn().mockResolvedValue({ dirty: true }), vi.fn().mockResolvedValue(false), { confirming: false }, release)
    expect(release).toHaveBeenCalledOnce()
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
