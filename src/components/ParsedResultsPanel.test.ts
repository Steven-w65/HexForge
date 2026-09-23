import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ParsedField } from '../types'
import ParsedResultsPanel from './ParsedResultsPanel.vue'

describe('ParsedResultsPanel', () => {
  it('renders compact result cards and emits the exact inclusive byte range', async () => {
    const result: ParsedField = { name: 'magic', offset: '16', type: 'u32', length: 4, endianness: 'little', value: '0x1234', comment: 'Header' }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [result], collapsed: false, hasFile: true, templateHasFields: true } })
    const card = wrapper.get('[data-testid="parsed-result"]')
    expect(card.get('.result-name').text()).toBe('magic')
    expect(card.get('.result-value').text()).toBe('0x1234')
    expect(card.get('.result-meta').text()).toContain('0x00000010 · u32 · LE')
    expect(card.get('.result-comment').text()).toBe('Header')
    await card.trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 16n, end: 19n }]])
  })

  it.each([
    [{ hasFile: false, templateHasFields: false }, 'Open a binary file', 'open'],
    [{ hasFile: true, templateHasFields: false }, 'Create or load', 'template-editor'],
    [{ hasFile: true, templateHasFields: true }, 'ready to apply', 'apply-template'],
  ] as const)('offers a contextual empty action for %o', async (state, copy, command) => {
    const wrapper = mount(ParsedResultsPanel, { props: { results: [], collapsed: false, ...state } })
    expect(wrapper.get('[data-testid="results-empty"]').text()).toContain(copy)
    await wrapper.get('[data-action="results-empty-action"]').trigger('click')
    expect(wrapper.emitted('empty-action')).toEqual([[command]])
  })
})
