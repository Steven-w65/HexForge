export type MenuCommand =
  | 'open' | 'close-file' | 'save-as' | 'export' | 'exit'
  | 'edit-selected' | 'toggle-edit' | 'undo'
  | 'goto' | 'search'
  | 'template-editor' | 'apply-template' | 'load-template' | 'unload-template' | 'save-template' | 'save-template-as'
  | 'theme-toggle'
  | 'row-16' | 'row-32' | 'toggle-right-panel'
  | 'minimap-toggle' | 'minimap-fit' | 'minimap-proportional' | 'minimap-characters' | 'minimap-blocks'
  | 'minimap-scale-1' | 'minimap-scale-2' | 'minimap-scale-3'

export interface MenuState {
  hasFile: boolean
  hasBytes: boolean
  singleByteSelected: boolean
  editMode: boolean
  canUndo: boolean
  templateValid: boolean
  templateActive: boolean
  templateHasPath: boolean
  templateHasFields: boolean
  hasParsedResults: boolean
  operationBusy: boolean
  templateIssueCount?: number
}

interface Shortcut {
  command: MenuCommand
  key: string
  ctrl?: boolean
  shift?: boolean
  alt?: boolean
}

const shortcuts: Shortcut[] = [
  { command: 'open', key: 'o', ctrl: true },
  { command: 'close-file', key: 'w', ctrl: true },
  { command: 'save-as', key: 's', ctrl: true, alt: true },
  { command: 'export', key: 'e', ctrl: true, shift: true },
  { command: 'exit', key: 'f4', alt: true },
  { command: 'edit-selected', key: 'f2' },
  { command: 'toggle-edit', key: 'e', ctrl: true, alt: true },
  { command: 'undo', key: 'z', ctrl: true },
  { command: 'goto', key: 'g', ctrl: true },
  { command: 'search', key: 'f', ctrl: true },
  { command: 'template-editor', key: 't', ctrl: true, shift: true },
  { command: 'apply-template', key: 'enter', ctrl: true },
  { command: 'load-template', key: 'o', ctrl: true, alt: true },
  { command: 'unload-template', key: 'u', ctrl: true, alt: true },
  { command: 'save-template', key: 's', ctrl: true },
  { command: 'save-template-as', key: 's', ctrl: true, shift: true },
  { command: 'theme-toggle', key: 't', ctrl: true, alt: true },
  { command: 'row-16', key: '1', ctrl: true },
  { command: 'row-32', key: '2', ctrl: true },
]

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
}

export function matchMenuShortcut(event: KeyboardEvent, target: EventTarget | null = event.target): MenuCommand | null {
  const key = event.key.toLowerCase()
  const shortcut = shortcuts.find((candidate) => candidate.key === key &&
    Boolean(candidate.ctrl) === event.ctrlKey && Boolean(candidate.shift) === event.shiftKey &&
    Boolean(candidate.alt) === event.altKey && !event.metaKey)
  if (!shortcut) return null
  if (isEditable(target) && (shortcut.command === 'undo' || shortcut.command === 'edit-selected')) return null
  return shortcut.command
}

/** The same rule and reason drive menus, editor buttons, shortcuts and dialogs. */
export function commandUnavailableReason(command: MenuCommand, state: MenuState): string | null {
  if (command === 'exit' || command === 'theme-toggle' || command.startsWith('row-') || command.startsWith('minimap-') || command === 'toggle-right-panel') return null
  if (state.operationBusy) return 'Wait for the current operation to finish.'
  const invalid = state.templateIssueCount ? `Correct ${state.templateIssueCount} template ${state.templateIssueCount === 1 ? 'error' : 'errors'} first.` : 'Correct the highlighted template errors first.'
  switch (command) {
    case 'open':
    case 'template-editor':
    case 'load-template':
      return null
    case 'save-template':
      if (!state.templateActive) return 'Load or create a template first.'
      if (!state.templateHasPath) return 'Use Save As first to choose a template file.'
      return state.templateValid ? null : invalid
    case 'save-template-as':
      if (!state.templateActive) return 'Load or create a template first.'
      return state.templateValid ? null : invalid
    case 'unload-template':
      return state.templateActive ? null : 'No template is loaded.'
    case 'close-file':
    case 'save-as':
    case 'toggle-edit':
      return state.hasFile ? null : 'Open a binary file first.'
    case 'goto':
    case 'search':
      return state.hasBytes ? null : 'Open a nonempty binary file first.'
    case 'edit-selected':
      if (!state.hasBytes) return 'Open a nonempty binary file first.'
      if (!state.editMode) return 'Enable Edit Mode first.'
      return state.singleByteSelected ? null : 'Select one byte to edit.'
    case 'undo':
      return state.hasFile && state.canUndo ? null : 'No byte edit to undo.'
    case 'export':
      return state.hasFile && state.hasParsedResults ? null : 'Apply a valid template before exporting results.'
    case 'apply-template':
      if (!state.hasFile) return 'Open a binary file to apply the template.'
      if (!state.templateValid) return invalid
      return state.templateHasFields ? null : 'Add a template field first.'
  }
  return 'This action is unavailable.'
}

export function commandEnabled(command: MenuCommand, state: MenuState): boolean { return commandUnavailableReason(command, state) === null }
