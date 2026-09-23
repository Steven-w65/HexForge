import { matchMenuShortcut, type MenuCommand } from '../menu/commands'

export interface HotkeyActions {
  invoke(command: MenuCommand): void
  isEnabled(command: MenuCommand): boolean
  isPopupOpen(): boolean
  closePopup(): void
  clearSelection(): void
}

export function useHotkeys(actions: HotkeyActions, target: Window = window): () => void {
  const listener = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault()
      if (actions.isPopupOpen()) actions.closePopup()
      else actions.clearSelection()
      return
    }
    const command = matchMenuShortcut(event)
    if (!command || !actions.isEnabled(command)) return
    event.preventDefault()
    actions.invoke(command)
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
