import { enableAutoUnmount, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import AppDialog from './AppDialog.vue'

enableAutoUnmount(afterEach)

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

  it('traps forward and backward tab focus and restores the invoking control', async () => {
    const opener = document.createElement('button')
    document.body.append(opener); opener.focus()
    const wrapper = mount(AppDialog, { attachTo: document.body, props: { open: false },
      slots: { default: '<button data-testid="save">Save</button><button data-testid="cancel" autofocus>Cancel</button>' } })
    await wrapper.setProps({ open: true }); await nextTick()
    const first = wrapper.get('[aria-label="Close dialog"]')
    const last = wrapper.get('[data-testid="cancel"]')
    ;(last.element as HTMLElement).focus()
    await last.trigger('keydown', { key: 'Tab' })
    expect(document.activeElement).toBe(first.element)
    await first.trigger('keydown', { key: 'Tab', shiftKey: true })
    expect(document.activeElement).toBe(last.element)
    await wrapper.setProps({ open: false }); await nextTick()
    expect(document.activeElement).toBe(opener)
    wrapper.unmount()
  })

  it('makes content outside the dialog inert and respects a nested dialog', async () => {
    const app = document.createElement('main'); app.innerHTML = '<button>Underlying app</button>'; document.body.append(app)
    const wrapper = mount(AppDialog, { attachTo: app, props: { open: true }, slots: { default: '<button autofocus>Cancel</button>' } })
    await nextTick()
    expect(app.querySelector('[role="dialog"]')).not.toBeNull()
    // The dialog itself stays interactive; only its outside siblings are inert.
    expect(app.querySelector('button')?.hasAttribute('inert')).toBe(true)
    const nested = mount(AppDialog, { attachTo: app, props: { open: true, title: 'Error' } })
    await nextTick()
    expect(wrapper.get('.dialog-backdrop').element.closest('[inert]')).not.toBeNull()
    await nested.setProps({ open: false }); await nextTick()
    expect(wrapper.get('.dialog-backdrop').element.closest('[inert]')).toBeNull()
    wrapper.unmount(); nested.unmount()
    expect(app.querySelector('button')?.hasAttribute('inert')).toBe(false)
  })
})
