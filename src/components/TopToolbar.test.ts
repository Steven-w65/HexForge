import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TopToolbar from './TopToolbar.vue'
import source from './TopToolbar.vue?raw'
import tauriConfig from '../../src-tauri/tauri.conf.json'

describe('TopToolbar', () => {
  it('keeps the required action order and forwards each action', async () => {
    const wrapper = mount(TopToolbar, { props: { hasFile: true, dirty: true, editMode: false, theme: 'dark', templateValid: true } })
    const buttons = wrapper.findAll('[data-action]')
    expect(buttons.map((button) => button.attributes('data-action'))).toEqual([
      'open', 'goto', 'search', 'template', 'export', 'theme', 'edit', 'save-as',
    ])
    for (const button of buttons) await button.trigger('click')
    for (const action of ['open', 'goto', 'search', 'template', 'export', 'theme', 'edit', 'save-as']) {
      expect(wrapper.emitted(action)).toHaveLength(1)
    }
  })

  it('keeps Open enabled and disables file actions with no file', () => {
    const wrapper = mount(TopToolbar, { props: { hasFile: false, dirty: false, editMode: false, theme: 'dark' } })
    expect(wrapper.get('[data-action="open"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-action="search"]').attributes('disabled')).toBeDefined()
  })

  it('switches to the compact icon layout above Tauri minimum width', () => {
    const breakpoint = Number(source.match(/@media \(max-width: (\d+)px\)/)?.[1])
    expect(breakpoint).toBeGreaterThan(tauriConfig.app.windows[0]!.minWidth)
    expect(source).toContain('button span { display: none; }')
    const wrapper = mount(TopToolbar, { props: { hasFile: true, dirty: false, editMode: false, theme: 'dark' } })
    const required = Number(wrapper.get('.toolbar').attributes('style')?.match(/--compact-required-width: (\d+)px/)?.[1])
    expect(required).toBeLessThanOrEqual(tauriConfig.app.windows[0]!.minWidth)
    expect(source).toContain('overflow-x: auto')
    expect(wrapper.findAll('[data-action]')).toHaveLength(8)
  })
})
