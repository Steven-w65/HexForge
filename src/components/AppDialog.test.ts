import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import AppDialog from './AppDialog.vue'

describe('AppDialog', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('focuses the requested control whenever the dialog opens', async () => {
    const wrapper = mount(AppDialog, {
      attachTo: document.body,
      props: { open: false, title: 'Search bytes' },
      slots: { default: '<input data-testid="dialog-input" autofocus>' },
    })
    await wrapper.setProps({ open: true })
    await nextTick()
    expect(document.activeElement).toBe(wrapper.get('[data-testid="dialog-input"]').element)
    wrapper.unmount()
  })

  it('exposes one consistent close control and closes on the backdrop', async () => {
    const wrapper = mount(AppDialog, { props: { open: true, title: 'Go to offset' } })
    expect(wrapper.get('[role="dialog"]').attributes('aria-label')).toBe('Go to offset')
    await wrapper.get('[aria-label="Close dialog"]').trigger('click')
    await wrapper.get('.dialog-backdrop').trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(2)
  })
})
