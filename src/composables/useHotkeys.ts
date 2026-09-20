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
export interface AllowCloseGuard { value: boolean; confirming?: boolean }

export async function handleCloseRequest(
  event: CloseRequestEventLike,
  dirty: boolean,
  confirmDiscard: () => Promise<boolean>,
  close: () => void | Promise<void>,
  allowClose: AllowCloseGuard = { value: false },
): Promise<void> {
  if (allowClose.value) { allowClose.value = false; return }
  if (!dirty) return
  event.preventDefault()
  if (allowClose.confirming) return
  allowClose.confirming = true
  try {
    if (!await confirmDiscard()) return
    allowClose.value = true
    try { await close() }
    catch (error) { allowClose.value = false; throw error }
  } finally {
    allowClose.confirming = false
  }
}
