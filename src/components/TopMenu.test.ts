import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import type { MenuState } from '../menu/commands'
import type { ParsedField, TemplateDefinition } from '../types'
import TopMenu from './TopMenu.vue'

const state = (patch: Partial<MenuState> = {}): MenuState => ({
  hasFile: true, hasBytes: true, singleByteSelected: true, editMode: true, canUndo: true,
  templateValid: true, templateHasFields: true, hasNavigableTemplateFields: true,
  hasParsedResults: true, operationBusy: false, ...patch,
})

const template: TemplateDefinition = {
  version: 1, name: 'Header', defaultEndianness: 'little', fields: [
    { name: 'magic', offset: '0', type: 'bytes', length: 4, comment: '' },
    { name: 'version', offset: '0x4', type: 'u16', endianness: 'little', comment: '' },
  ],
}

const results: ParsedField[] = [
  { name: 'magic', offset: '0', type: 'bytes', length: 4, endianness: 'little', value: '48 45 58 46', comment: '' },
]

const mounted: VueWrapper[] = []

function mountMenu(menuState = state()) {
  const wrapper = mount(TopMenu, { attachTo: document.body, props: {
    state: menuState, bytesPerRow: 16,
    fileSize: 32n, template, results,
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
      navigate: ['goto', 'search'],
      template: ['template-editor', 'apply-template', 'load-template', 'save-template', 'add-field'],
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

  it('lists valid template fields and parsed results and emits their exact byte ranges', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    await wrapper.get('[data-submenu="template-fields"]').trigger('click')
    expect(wrapper.text()).toContain('version')
    await wrapper.get('[data-template-field="1"]').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 4n, end: 5n }]])
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    await wrapper.get('[data-submenu="parsed-results"]').trigger('click')
    await wrapper.get('[data-parsed-result="0"]').trigger('click')
    expect(wrapper.emitted('navigate')?.at(-1)).toEqual([{ start: 0n, end: 3n }])
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

  it('opens dynamic Navigate submenus through their access keys and invokes a selected range', async () => {
    const wrapper = mountMenu()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', altKey: true, bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.templateField).toBe('0')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.templateField).toBe('1')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 4n, end: 5n }]])

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', altKey: true, bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', bubbles: true, cancelable: true })); await wrapper.vm.$nextTick()
    expect((document.activeElement as HTMLElement).dataset.parsedResult).toBe('0')
  })

  it('opens Navigate children as flyouts and returns focus to the trigger with ArrowLeft', async () => {
    const wrapper = mountMenu()
    await wrapper.get('[data-menu="navigate"]').trigger('click')
    const trigger = wrapper.get('[data-submenu="template-fields"]')
    ;(trigger.element as HTMLElement).focus()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.get('[data-open-submenu="template-fields"]').classes()).toContain('submenu-flyout')
    expect((document.activeElement as HTMLElement).dataset.templateField).toBe('0')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[data-open-submenu="template-fields"]').exists()).toBe(false)
    expect(document.activeElement).toBe(trigger.element)
  })
})
