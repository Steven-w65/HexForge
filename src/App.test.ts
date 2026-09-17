import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import App from './App.vue'

describe('App', () => {
  it('renders the HexForge application landmark', () => {
    const wrapper = mount(App)
    expect(wrapper.get('[data-testid="hexforge-app"]').attributes('role')).toBe('application')
  })
})
