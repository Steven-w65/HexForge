export type MenuCommand =
  | 'open' | 'close-file' | 'save-as' | 'export' | 'exit'
  | 'edit-selected' | 'toggle-edit' | 'undo'
  | 'goto' | 'search'
  | 'template-editor' | 'apply-template' | 'load-template' | 'unload-template' | 'save-template' | 'add-field'
  | 'theme-toggle'
  | 'row-16' | 'row-32' | 'toggle-right-panel'

export interface MenuState {
  hasFile: boolean
  hasBytes: boolean
  singleByteSelected: boolean
  editMode: boolean
  canUndo: boolean
  templateValid: boolean
  templateActive: boolean
  templateHasFields: boolean
  hasNavigableTemplateFields: boolean
  hasParsedResults: boolean
  operationBusy: boolean
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
  { command: 'save-as', key: 's', ctrl: true, shift: true },
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
  { command: 'add-field', key: 'a', ctrl: true, alt: true },
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

export function commandEnabled(command: MenuCommand, state: MenuState): boolean {
  if (command === 'exit' || command === 'theme-toggle' || command.startsWith('row-') || command === 'toggle-right-panel') return true
  if (state.operationBusy) return false
  switch (command) {
    case 'open':
    case 'template-editor':
    case 'load-template':
    case 'add-field':
      return true
    case 'save-template':
      return state.templateValid
    case 'unload-template':
      return state.templateActive
    case 'close-file':
    case 'save-as':
    case 'toggle-edit':
      return state.hasFile
    case 'goto':
    case 'search':
      return state.hasBytes
    case 'edit-selected':
      return state.hasBytes && state.editMode && state.singleByteSelected
    case 'undo':
      return state.hasFile && state.canUndo
    case 'export':
      return state.hasFile && state.hasParsedResults
    case 'apply-template':
      return state.hasFile && state.templateValid && state.templateHasFields
  }
  return false
}
