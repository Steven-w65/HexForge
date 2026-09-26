import { describe, expect, it } from 'vitest'
import { commandEnabled, matchMenuShortcut, type MenuCommand, type MenuState } from './commands'

const readyState = (): MenuState => ({
  hasFile: true,
  hasBytes: true,
  singleByteSelected: true,
  editMode: true,
  canUndo: true,
  templateValid: true,
  templateActive: true,
  templateHasFields: true,
  hasNavigableTemplateFields: true,
  hasParsedResults: true,
  operationBusy: false,
})

function shortcut(key: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, cancelable: true, ...modifiers })
}

describe('menu commands', () => {
  it.each([
    ['o', { ctrlKey: true }, 'open'],
    ['w', { ctrlKey: true }, 'close-file'],
    ['s', { ctrlKey: true, shiftKey: true }, 'save-as'],
    ['e', { ctrlKey: true, shiftKey: true }, 'export'],
    ['F4', { altKey: true }, 'exit'],
    ['F2', {}, 'edit-selected'],
    ['e', { ctrlKey: true, altKey: true }, 'toggle-edit'],
    ['z', { ctrlKey: true }, 'undo'],
    ['g', { ctrlKey: true }, 'goto'],
    ['f', { ctrlKey: true }, 'search'],
    ['t', { ctrlKey: true, shiftKey: true }, 'template-editor'],
    ['Enter', { ctrlKey: true }, 'apply-template'],
    ['o', { ctrlKey: true, altKey: true }, 'load-template'],
    ['u', { ctrlKey: true, altKey: true }, 'unload-template'],
    ['s', { ctrlKey: true }, 'save-template'],
    ['a', { ctrlKey: true, altKey: true }, 'add-field'],
    ['t', { ctrlKey: true, altKey: true }, 'theme-toggle'],
    ['1', { ctrlKey: true }, 'row-16'],
    ['2', { ctrlKey: true }, 'row-32'],
  ] satisfies Array<[string, KeyboardEventInit, MenuCommand]>)('maps %s with exact modifiers to %s', (key, modifiers, command) => {
    expect(matchMenuShortcut(shortcut(key, modifiers))).toBe(command)
  })

  it('does not treat shifted or extra-modifier variants as another command', () => {
    expect(matchMenuShortcut(shortcut('o', { ctrlKey: true, shiftKey: true }))).toBeNull()
    expect(matchMenuShortcut(shortcut('b', { ctrlKey: true, shiftKey: true }))).toBeNull()
    expect(matchMenuShortcut(shortcut('F2', { altKey: true }))).toBeNull()
  })

  it('does not bind removed theme-selection or panel-visibility shortcuts', () => {
    expect(matchMenuShortcut(shortcut('d', { ctrlKey: true, altKey: true }))).toBeNull()
    expect(matchMenuShortcut(shortcut('l', { ctrlKey: true, altKey: true }))).toBeNull()
    expect(matchMenuShortcut(shortcut('b', { ctrlKey: true }))).toBeNull()
    expect(matchMenuShortcut(shortcut('b', { ctrlKey: true, altKey: true }))).toBeNull()
  })

  it('keeps text-editing undo and F2 inside editable controls', () => {
    const input = document.createElement('input')
    expect(matchMenuShortcut(shortcut('z', { ctrlKey: true }), input)).toBeNull()
    expect(matchMenuShortcut(shortcut('F2'), input)).toBeNull()
    expect(matchMenuShortcut(shortcut('s', { ctrlKey: true }), input)).toBe('save-template')
  })

  it('disables file-dependent commands without a file while leaving template and view commands available', () => {
    const state: MenuState = {
      ...readyState(), hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false,
      canUndo: false, templateHasFields: false, hasNavigableTemplateFields: false, hasParsedResults: false,
    }
    for (const command of ['close-file', 'save-as', 'export', 'edit-selected', 'toggle-edit', 'undo', 'goto', 'search', 'apply-template'] as const) {
      expect(commandEnabled(command, state), command).toBe(false)
    }
    for (const command of ['open', 'template-editor', 'load-template', 'save-template', 'add-field', 'theme-toggle', 'row-16', 'row-32', 'toggle-right-panel', 'exit'] as const) {
      expect(commandEnabled(command, state), command).toBe(true)
    }
  })

  it('enforces selection, undo, result, template-validity and busy-state requirements', () => {
    expect(commandEnabled('unload-template', { ...readyState(), templateActive: false })).toBe(false)
    expect(commandEnabled('unload-template', readyState())).toBe(true)
    expect(commandEnabled('edit-selected', { ...readyState(), singleByteSelected: false })).toBe(false)
    expect(commandEnabled('edit-selected', { ...readyState(), editMode: false })).toBe(false)
    expect(commandEnabled('undo', { ...readyState(), canUndo: false })).toBe(false)
    expect(commandEnabled('export', { ...readyState(), hasParsedResults: false })).toBe(false)
    expect(commandEnabled('apply-template', { ...readyState(), templateValid: false })).toBe(false)
    expect(commandEnabled('apply-template', { ...readyState(), templateHasFields: false })).toBe(false)
    expect(commandEnabled('save-template', { ...readyState(), templateValid: false })).toBe(false)
    expect(commandEnabled('open', { ...readyState(), operationBusy: true })).toBe(false)
    expect(commandEnabled('theme-toggle', { ...readyState(), operationBusy: true })).toBe(true)
    expect(commandEnabled('exit', { ...readyState(), operationBusy: true })).toBe(true)
  })
})
