import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ParsedField } from '../types'
import ParsedResultsPanel from './ParsedResultsPanel.vue'

describe('ParsedResultsPanel', () => {
  it('shows flat result columns in the bottom pane while preserving exact navigation', async () => {
    const result: ParsedField = { name: 'magic', offset: '16', type: 'u32', length: 4, endianness: 'little', value: '0x1234', comment: 'Header' }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [result], collapsed: false, hasFile: true, templateHasFields: true } })
    const row = wrapper.get('[data-testid="parsed-result"]')
    expect(wrapper.get('.result-columns').text()).toBe('NameValueOffsetSizeType')
    expect(row.findAll('.result-cell').map((cell) => cell.text())).toEqual(['magic', '0x1234', '0x00000010', '4 B', 'u32 · LE'])
    expect(row.attributes('title')).toBe('Header')
    await row.trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 16n, end: 19n }]])
  })

  it('keeps the bottom pane header visible when results are collapsed', async () => {
    const wrapper = mount(ParsedResultsPanel, { props: { results: [], collapsed: true } })
    expect(wrapper.get('.results-header').text()).toContain('PARSED RESULTS')
    expect(wrapper.find('.results-content').exists()).toBe(false)
    await wrapper.get('[data-action="collapse-results"]').trigger('click')
    expect(wrapper.emitted('toggle-collapse')).toEqual([[]])
  })

  it('marks only the result matching the current template highlight as active', async () => {
    const fields: ParsedField[] = [
      { name: 'first', offset: '16', type: 'u32', length: 4, endianness: 'little', value: '1', comment: '' },
      { name: 'second', offset: '20', type: 'u8', length: 1, endianness: 'little', value: '2', comment: '' },
    ]
    const wrapper = mount(ParsedResultsPanel, { props: {
      results: fields, collapsed: false, templateRange: { start: 16n, end: 19n, count: 4n },
    } })
    const rows = wrapper.findAll('[data-testid="parsed-result"]')
    expect(rows[0]!.classes()).toContain('active')
    expect(rows[1]!.classes()).not.toContain('active')
    await wrapper.setProps({ templateRange: { start: 20n, end: 20n, count: 1n } })
    expect(rows[0]!.classes()).not.toContain('active')
    expect(rows[1]!.classes()).toContain('active')
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
