import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import { matchMenuShortcut, type MenuCommand, type MenuState } from '../menu/commands'
import type { ParsedNode } from '../types'
import TopMenu from './TopMenu.vue'
import { lockModalFocus } from '../ui/modalFocus'

const state = (patch: Partial<MenuState> = {}): MenuState => ({
  hasFile: true, hasBytes: true, singleByteSelected: true, editMode: true, canUndo: true,
  templateValid: true, templateActive: true, templateHasFields: true,
  templateHasPath: true,
  hasParsedResults: true, operationBusy: false, ...patch,
})

const results: ParsedNode[] = [
  { kind: 'leaf', name: 'magic', path: 'magic', offset: '0', type: 'bytes', length: '4', endianness: null, value: '48 45 58 46', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] },
]

const mounted: VueWrapper[] = []

function mountMenu(menuState = state()) {
  const wrapper = mount(TopMenu, { attachTo: document.body, props: {
    state: menuState, bytesPerRow: 16,
    results,
  } })
  mounted.push(wrapper)
  return wrapper
}

describe('TopMenu', () => {
  afterEach(() => mounted.splice(0).forEach((wrapper) => wrapper.unmount()))

  it('renders the five requested top-level menus and every fixed command in its logical group', async () => {
    const wrapper = mountMenu()
    expect(wrapper.get('.menu-brand').text()).toContain('HexForge')
    expect(wrapper.find('.dirty-dot').exists()).toBe(false)
    expect(wrapper.findAll('[role="menubar"] > button').map((item) => item.text())).toEqual(['File', 'Edit', 'Navigate', 'Template', 'View'])
    const expected: Record<string, string[]> = {
      file: ['open', 'close-file', 'save-as', 'export', 'exit'],
      edit: ['edit-selected', 'toggle-edit', 'undo'],
      navigate: ['goto', 'search', 'search-next', 'search-previous'],
      template: ['template-editor', 'apply-template', 'load-template', 'unload-template', 'save-template', 'save-template-as'],
      view: ['theme-toggle', 'row-16', 'row-32'],
    }
    for (const [menu, commands] of Object.entries(expected)) {
      await wrapper.get(`[data-menu="${menu}"]`).trigger('click')
      expect(wrapper.findAll('[data-menu-command]').map((item) => item.attributes('data-menu-command'))).toEqual(commands)
    }
  })

  it('exposes the separated Template Editor with its keyboard shortcut', async () => {
    const wrapper = mountMenu(state({ hasFile: false, templateHasFields: false }))
    await wrapper.get('[data-menu="template"]').trigger('click')
    const editor = wrapper.get('[data-menu-command="template-editor"]')
    expect(editor.attributes('disabled')).toBeUndefined()
    expect(editor.get('kbd').text()).toBe('Ctrl+Shift+T')
    await editor.trigger('click')
    expect(wrapper.emitted('command')).toEqual([['template-editor']])
  })

  it('shows the exact working shortcut for every top-navigation command, with no phantom save bindings', async () => {
    const wrapper = mountMenu()
    const expected: Partial<Record<MenuCommand, string>> = {
      open: 'Ctrl+O', 'close-file': 'Ctrl+W', 'save-as': 'Ctrl+Shift+S', export: 'Ctrl+Shift+E', exit: 'Alt+F4',
      'edit-selected': 'F2', 'toggle-edit': 'Ctrl+Alt+E', undo: 'Ctrl+Z',
      goto: 'Ctrl+G', search: 'Ctrl+F', 'search-next': 'F3', 'search-previous': 'Shift+F3',
      'template-editor': 'Ctrl+Shift+T', 'apply-template': 'Ctrl+Enter', 'load-template': 'Ctrl+Alt+O', 'unload-template': 'Ctrl+Alt+U',
      'theme-toggle': 'Ctrl+Alt+T', 'row-16': 'Ctrl+1', 'row-32': 'Ctrl+2',
    }
    const seen = new Set<MenuCommand>()
    for (const menu of ['file', 'edit', 'navigate', 'template', 'view']) {
      await wrapper.get(`[data-menu="${menu}"]`).trigger('click')
      for (const item of wrapper.findAll('[data-menu-command]')) {
        const command = item.attributes('data-menu-command') as MenuCommand
        seen.add(command)
        const label = expected[command]
        if (!label) { expect(item.find('kbd').exists(), command).toBe(false); continue }
        expect(item.get('kbd').text(), command).toBe(label)
        const keys = label.split('+')
        expect(matchMenuShortcut(new KeyboardEvent('keydown', {
          key: keys.at(-1), ctrlKey: keys.includes('Ctrl'), altKey: keys.includes('Alt'), shiftKey: keys.includes('Shift'),
        })), label).toBe(command)
      }
    }
    for (const command of Object.keys(expected) as MenuCommand[]) expect(seen.has(command), command).toBe(true)
  })

  it('disables in-place template Save without a path while keeping Save As accessible', async () => {
    const wrapper = mountMenu(state({ templateHasPath: false }))
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="save-template"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-menu-command="save-template-as"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-menu-command="save-template-as"]').find('kbd').exists()).toBe(false)
  })

  it('advertises binary Save As without assigning its main-window keys to template saves', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="file"]').trigger('click')
    const saveAs = wrapper.get('[data-menu-command="save-as"]')
    expect(saveAs.get('kbd').text()).toBe('Ctrl+Shift+S')
    await saveAs.trigger('click')
    expect(wrapper.emitted('command')).toEqual([['save-as']])
    await wrapper.get('[data-menu="template"]').trigger('click')
    expect(wrapper.get('[data-menu-command="save-template"]').find('kbd').exists()).toBe(false)
    expect(wrapper.get('[data-menu-command="save-template-as"]').find('kbd').exists()).toBe(false)
    await wrapper.get('[data-menu-command="save-template"]').trigger('click')
    expect(wrapper.emitted('command')?.at(-1)).toEqual(['save-template'])
  })

  it('emits enabled commands and leaves disabled commands inert', async () => {
    const wrapper = mountMenu(state({ hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false, canUndo: false, hasParsedResults: false }))
    await wrapper.get('[data-menu="file"]').trigger('click')
    expect(wrapper.get('[data-menu-command="close-file"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-menu-command="close-file"]').trigger('click')
    expect(wrapper.emitted('command')).toBeUndefined()
    await wrapper.get('[data-menu-command="open"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['open']])
  })

  it('marks active edit and row-width states', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="edit"]').trigger('click')
    expect(wrapper.get('[data-menu-command="toggle-edit"]').attributes('aria-checked')).toBe('true')
    await wrapper.get('[data-menu="view"]').trigger('click')
    expect(wrapper.get('[data-menu-command="theme-toggle"] kbd').text()).toBe('Ctrl+Alt+T')
    expect(wrapper.get('[data-menu-command="row-16"]').attributes('aria-checked')).toBe('true')
    expect(wrapper.get('[data-menu-command="row-32"]').attributes('aria-checked')).toBe('false')
  })

  it('offers minimap settings without column-width choices in the View submenu', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="view"]').trigger('click')
    await wrapper.get('[data-submenu="minimap"]').trigger('click')
    expect(wrapper.findAll('[data-minimap-command]').map((item) => item.attributes('data-minimap-command'))).toEqual([
      'minimap-toggle', 'minimap-fit', 'minimap-proportional', 'minimap-characters', 'minimap-blocks',
      'minimap-scale-1', 'minimap-scale-2', 'minimap-scale-3',
    ])
    expect(wrapper.get('[data-minimap-command="minimap-fit"]').attributes('aria-checked')).toBe('true')
    expect(wrapper.get('[data-minimap-command="minimap-toggle"]').find('kbd').exists()).toBe(false)
    await wrapper.get('[data-minimap-command="minimap-proportional"]').trigger('click')
    expect(wrapper.emitted('command')).toEqual([['minimap-proportional']])
  })

  it('opens the minimap flyout from the keyboard and restores focus on ArrowLeft', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="view"]').trigger('click')
    const trigger = wrapper.get('[data-submenu="minimap"]')
    ;(trigger.element as HTMLElement).focus()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.minimapCommand).toBe('minimap-toggle')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(document.activeElement).toBe(trigger.element)
  })

  it('positions a pointer-opened dropdown below its menu heading', async () => {
    const wrapper = mountMenu()
    const viewButton = wrapper.get('[data-menu="view"]')
    Object.defineProperty(viewButton.element, 'offsetLeft', { configurable: true, value: 284 })
    await viewButton.trigger('click')
    expect(wrapper.get('.menu-popup').attributes('style')).toContain('left: 284px')
  })

  it('re-anchors keyboard-opened and arrow-switched dropdowns to their headings', async () => {
    const wrapper = mountMenu()
    Object.defineProperty(wrapper.get('[data-menu="file"]').element, 'offsetLeft', { configurable: true, value: 92 })
    Object.defineProperty(wrapper.get('[data-menu="edit"]').element, 'offsetLeft', { configurable: true, value: 133 })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('.menu-popup').attributes('style')).toContain('left: 92px')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('.menu-popup').attributes('style')).toContain('left: 133px')
  })

  it('offers parsed-result navigation without a Template Fields submenu', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    expect(wrapper.findAll('[data-submenu]').map((item) => item.attributes('data-submenu'))).toEqual(['parsed-results'])
    await wrapper.get('[data-submenu="parsed-results"]').trigger('click')
    await wrapper.get('[data-parsed-result="0"]').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 0n, end: 3n }]])
  })

  it('opens top-level menus with Alt access keys and closes them with Escape', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('[data-menu="file"]').attributes('aria-expanded')).toBe('true')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('[data-menu="file"]').attributes('aria-expanded')).toBe('false')
  })

  it('navigates menu items with arrow keys and invokes the focused command with Enter', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.menuCommand).toBe('open')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.menuCommand).toBe('close-file')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect(wrapper.emitted('command')).toEqual([['close-file']])
  })

  it('does not turn modified Enter into a click on the focused menu command', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.menuCommand).toBe('open')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.emitted('command')).toBeUndefined()
  })

  it('does not change menus or steal Escape from a foreground modal', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    const modal = document.createElement('section')
    modal.innerHTML = '<button>Cancel</button>'
    document.body.append(modal)
    const release = lockModalFocus(modal, null)
    try {
      const access = new KeyboardEvent('keydown', { key: 'v', altKey: true, bubbles: true, cancelable: true })
      window.dispatchEvent(access)
      const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
      window.dispatchEvent(escape)
      await wrapper.vm.$nextTick()
      expect(wrapper.get('[data-menu="file"]').attributes('aria-expanded')).toBe('true')
      expect(wrapper.get('[data-menu="view"]').attributes('aria-expanded')).toBe('false')
      expect(access.defaultPrevented).toBe(false)
      expect(escape.defaultPrevented).toBe(false)
      expect(wrapper.emitted('command')).toBeUndefined()
    } finally { release(); modal.remove() }
  })

  it('does not open a menu during IME composition', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', altKey: true, isComposing: true, bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[role="menu"]').exists()).toBe(false)
  })

  it('opens parsed results from the Navigate access key and invokes a selected range', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', altKey: true, bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.parsedResult).toBe('0')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 0n, end: 3n }]])
  })

  it('opens Navigate children as flyouts and returns focus to the trigger with ArrowLeft', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    const trigger = wrapper.get('[data-submenu="parsed-results"]')
    ;(trigger.element as HTMLElement).focus()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('[data-open-submenu="parsed-results"]').classes()).toContain('submenu-flyout')
    expect((document.activeElement as HTMLElement).dataset.parsedResult).toBe('0')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[data-open-submenu="parsed-results"]').exists()).toBe(false)
    expect(document.activeElement).toBe(trigger.element)
  })
})
