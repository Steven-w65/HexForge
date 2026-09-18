import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TopToolbar from './TopToolbar.vue'

describe('TopToolbar', () => {
  it('keeps the required action order and forwards each action', async () => {
    const wrapper = mount(TopToolbar, { props: { hasFile: true, dirty: true, editMode: false, theme: 'dark' } })
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
})
