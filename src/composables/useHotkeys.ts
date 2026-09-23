export interface HotkeyActions {
  open(): void
  search(): void
  goTo(): void
  saveTemplate(): void
  undo(): void
  isPopupOpen(): boolean
  closePopup(): void
  clearSelection(): void
}

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
}

export function useHotkeys(actions: HotkeyActions, target: Window = window): () => void {
  const listener = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault()
      if (actions.isPopupOpen()) actions.closePopup()
      else actions.clearSelection()
      return
    }
    if (!event.ctrlKey || event.shiftKey || event.altKey || event.metaKey) return
    const shortcuts: Record<string, () => void> = { o: actions.open, f: actions.search, g: actions.goTo, s: actions.saveTemplate, z: actions.undo }
    const action = shortcuts[event.key.toLowerCase()]
    if (!action) return
    if (event.key.toLowerCase() === 'z' && isEditable(event.target)) return
    event.preventDefault()
    action()
  }
  target.addEventListener('keydown', listener)
  return () => target.removeEventListener('keydown', listener)
}

export interface CloseRequestEventLike { preventDefault(): void }
export interface CloseRequestGuard { confirming?: boolean }

export async function handleCloseRequest(
  event: CloseRequestEventLike,
  getDirtyState: () => Promise<{ dirty: boolean }>,
  confirmDiscard: () => Promise<boolean>,
  guard: CloseRequestGuard = {},
  releaseBarrier: () => void = () => {},
): Promise<void> {
  if (guard.confirming) { event.preventDefault(); return }
  guard.confirming = true
  try {
    const { dirty } = await getDirtyState()
    if (dirty && !await confirmDiscard()) {
      event.preventDefault()
      releaseBarrier()
    }
  } catch (error) {
    event.preventDefault()
    releaseBarrier()
    throw error
  } finally {
    guard.confirming = false
  }
}
