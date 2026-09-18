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

  it('clears selection with Escape when no popup is open', () => {
    const target = actions(); cleanups.push(useHotkeys(target)); keydown('Escape')
    expect(target.clearSelection).toHaveBeenCalledOnce()
  })

  it('prevents close until dirty discard is confirmed and allows exactly one retry', async () => {
    const event = { preventDefault: vi.fn() }; const close = vi.fn(); const guard = { value: false }
    await handleCloseRequest(event, true, vi.fn().mockResolvedValue(false), close, guard)
    expect(event.preventDefault).toHaveBeenCalledOnce(); expect(close).not.toHaveBeenCalled()
    await handleCloseRequest(event, true, vi.fn().mockResolvedValue(true), close, guard)
    expect(close).toHaveBeenCalledOnce(); expect(guard.value).toBe(true)
    const retry = { preventDefault: vi.fn() }
    await handleCloseRequest(retry, true, vi.fn(), close, guard)
    expect(retry.preventDefault).not.toHaveBeenCalled(); expect(guard.value).toBe(false)
  })
})
