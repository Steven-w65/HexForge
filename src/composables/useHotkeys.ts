import { matchMenuShortcut, type MenuCommand } from '../menu/commands'
import { isModalOpen } from '../ui/modalFocus'

export interface HotkeyActions {
  invoke(command: MenuCommand): void
  isEnabled(command: MenuCommand): boolean
  isPopupOpen(): boolean
  closePopup(): void
  clearSelection(): void
}

export function useHotkeys(actions: HotkeyActions, target: Window = window): () => void {
  const listener = (event: KeyboardEvent): void => {
    // Composition keys belong to the input method, not application commands.
    if (event.isComposing) return
    // Ctrl+S belongs to the separate Template Editor. Consume it here without
    // dispatching a save so the WebView cannot open its Save Page dialog.
    if (event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === 's') {
      event.preventDefault()
      return
    }
    if (isModalOpen()) {
      if (matchMenuShortcut(event)) event.preventDefault()
      return // The top dialog owns Escape; never mutate the background selection/draft.
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      if (actions.isPopupOpen()) actions.closePopup()
      else actions.clearSelection()
      return
    }
    const command = matchMenuShortcut(event)
    if (!command) return
    // Reserved app shortcuts stay inert when disabled. Do not fall through to
    // browser Open/Find/Save dialogs or Enter activation of a focused button.
    event.preventDefault()
    if (!actions.isEnabled(command)) return
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
  onApproved: () => Promise<void> = async () => {},
): Promise<void> {
  if (guard.confirming) { event.preventDefault(); return }
  guard.confirming = true
  try {
    const { dirty } = await getDirtyState()
    if (dirty && !await confirmDiscard()) {
      event.preventDefault()
      releaseBarrier()
      return
    }
    await onApproved()
  } catch (error) {
    event.preventDefault()
    releaseBarrier()
    throw error
  } finally {
    guard.confirming = false
  }
}
