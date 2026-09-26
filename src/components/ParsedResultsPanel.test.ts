import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ParsedField } from '../types'
import ParsedResultsPanel from './ParsedResultsPanel.vue'

describe('ParsedResultsPanel', () => {
  it('shows flat result columns in the bottom pane while preserving exact navigation', async () => {
    const result: ParsedField = { name: 'magic', offset: '16', type: 'u32', length: 4, endianness: 'little', value: '0x1234', comment: 'Header' }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [result], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
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
      results: fields, collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true,
      templateRange: { start: 16n, end: 19n, count: 4n },
    } })
    const rows = wrapper.findAll('[data-testid="parsed-result"]')
    expect(rows[0]!.classes()).toContain('active')
    expect(rows[1]!.classes()).not.toContain('active')
    await wrapper.setProps({ templateRange: { start: 20n, end: 20n, count: 1n } })
    expect(rows[0]!.classes()).not.toContain('active')
    expect(rows[1]!.classes()).toContain('active')
  })

  it.each([
    [{ hasFile: false, templateSource: 'none' }, 'No template loaded.'],
    [{ hasFile: false, templateSource: 'file', templateName: 'header.json' }, 'Template: "header.json" loaded. Open binary file to preview.'],
    [{ hasFile: true, templateSource: 'none' }, 'No template loaded. Load or create template from Template menu.'],
    [{ hasFile: false, templateSource: 'draft', templateName: 'Draft' }, 'Unsaved template draft: "Draft".'],
    [{ hasFile: true, templateSource: 'draft', templateName: 'Draft' }, 'Unsaved template draft: "Draft".'],
  ] as const)('shows the exact text and no controls for %o', (state, message) => {
    const wrapper = mount(ParsedResultsPanel, { props: { results: [], collapsed: false, ...state } })
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe(message)
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
  })

  it('offers exactly Template Editor and Apply Template for a loaded template awaiting application', async () => {
    const wrapper = mount(ParsedResultsPanel, { props: {
      results: [], collapsed: false, hasFile: true, templateSource: 'file', templateName: 'header.json', templateApplied: false, canApply: true,
    } })
    expect(wrapper.get('[data-testid="results-empty"] p').text()).toBe('Template: "header.json" loaded, pending apply.')
    expect(wrapper.findAll('.results-content button').map((button) => button.text())).toEqual(['Template Editor', 'Apply Template'])
    await wrapper.get('[data-action="results-template-editor"]').trigger('click')
    await wrapper.get('[data-action="results-apply-template"]').trigger('click')
    expect(wrapper.emitted('action')).toEqual([['template-editor'], ['apply-template']])
  })

  it('disables pending Apply when the existing command is unavailable', () => {
    const wrapper = mount(ParsedResultsPanel, { props: {
      results: [], collapsed: false, hasFile: true, templateSource: 'file', templateName: 'empty.json', canApply: false,
    } })
    expect(wrapper.get('[data-action="results-apply-template"]').attributes('disabled')).toBeDefined()
  })

  it('shows applied content without controls even when parsing yields zero rows', () => {
    const wrapper = mount(ParsedResultsPanel, { props: {
      results: [], collapsed: false, hasFile: true, templateSource: 'file', templateName: 'header.json', templateApplied: true,
    } })
    expect(wrapper.find('.result-list').exists()).toBe(true)
    expect(wrapper.find('[data-testid="results-empty"]').exists()).toBe(false)
    expect(wrapper.findAll('.results-content button')).toHaveLength(0)
  })

  it('keeps the current template and apply state visible when results are collapsed', async () => {
    const wrapper = mount(ParsedResultsPanel, { props: {
      results: [], collapsed: true, hasFile: true, templateSource: 'file', templateName: 'header.json', templateApplied: false,
    } })
    expect(wrapper.get('[data-testid="template-state"]').text()).toContain('header.json')
    expect(wrapper.get('[data-testid="template-state"]').text()).toContain('Pending apply')
    await wrapper.setProps({ templateApplied: true })
    expect(wrapper.get('[data-testid="template-state"]').text()).toContain('Applied')
  })
})
