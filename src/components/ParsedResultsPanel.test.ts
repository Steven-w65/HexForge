import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ParsedNode } from '../types'
import ParsedResultsPanel from './ParsedResultsPanel.vue'

function leaf(name: string, offset: string, length: string, value: string): ParsedNode {
  return { kind: 'leaf', name, path: name, type: 'u8', offset, length, value, endianness: 'little', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] }
}

describe('ParsedResultsPanel', () => {
  it('preserves a collapsed container when a new parse retains its path', async () => {
    const root: ParsedNode = { ...leaf('header', '0', '1', ''), kind: 'struct', type: 'struct', value: null, children: [{ ...leaf('id', '0', '1', '1'), path: 'header.id' }] }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [root], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    await wrapper.get('[data-result-path="header"]').trigger('click')
    await wrapper.setProps({ results: [{ ...root, children: [{ ...root.children[0]!, value: '2' }] }] })
    expect(wrapper.find('[data-result-path="header.id"]').exists()).toBe(false)
    expect(wrapper.get('[data-result-path="header"]').attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
  })

  it('starts a large array collapsed and renders only a bounded window after expanding', async () => {
    const root: ParsedNode = { ...leaf('items', '0', '1000', ''), kind: 'array', type: 'array', value: null,
      children: Array.from({ length: 1000 }, (_, index) => ({ ...leaf(String(index), String(index), '1', String(index)), path: `items[${index}]` })) }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [root], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    expect(wrapper.findAll('[data-testid="parsed-result"]')).toHaveLength(1)
    await wrapper.get('[data-result-path="items"]').trigger('click')
    expect(wrapper.findAll('[data-testid="parsed-result"]').length).toBeLessThan(60)
    const viewport = wrapper.get('.result-list')
    ;(viewport.element as HTMLElement).scrollTop = 28000
    await viewport.trigger('scroll')
    await wrapper.get('[data-result-path="items[999]"]').trigger('click')
    expect(wrapper.emitted('navigate')?.at(-1)).toEqual([{ start: 999n, end: 999n }])
    wrapper.unmount()
  })

  it('uses arrow keys for tree focus, collapse and expansion, and Enter for exact navigation', async () => {
    const root: ParsedNode = { ...leaf('header', '0', '8', ''), kind: 'struct', type: 'struct', value: null,
      children: [{ ...leaf('id', '9007199254740993', '8', '42'), path: 'header.id' }] }
    const wrapper = mount(ParsedResultsPanel, { attachTo: document.body, props: { results: [root], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    await wrapper.get('[data-result-path="header"]').trigger('keydown', { key: 'ArrowDown' })
    expect((document.activeElement as HTMLElement)?.dataset.resultPath).toBe('header.id')
    await wrapper.get('[data-result-path="header.id"]').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('navigate')?.at(-1)).toEqual([{ start: 9007199254740993n, end: 9007199254741000n }])
    await wrapper.get('[data-result-path="header.id"]').trigger('keydown', { key: 'ArrowLeft' })
    await wrapper.get('[data-result-path="header"]').trigger('keydown', { key: 'ArrowLeft' })
    expect(wrapper.find('[data-result-path="header.id"]').exists()).toBe(false)
    await wrapper.get('[data-result-path="header"]').trigger('keydown', { key: 'ArrowRight' })
    expect(wrapper.find('[data-result-path="header.id"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('shows full diagnostic details without navigating an incomplete value', async () => {
    const message = 'payload: requested bytes exceed the declared length bound. '.repeat(20)
    const issue: ParsedNode = { ...leaf('payload', '10', '8', ''), kind: 'error', value: null, diagnostics: [{ code: 'invalid_template', message }] }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [issue], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    await wrapper.get('[data-result-path="payload"]').trigger('click')
    expect(wrapper.get('[data-testid="diagnostic-details"] p').text()).toBe(message.trim())
    expect(wrapper.emitted('navigate')).toBeUndefined()
    wrapper.unmount()
  })
  it('expands containers, navigates exact leaf bytes, and displays field diagnostics', async () => {
    const id: ParsedNode = { kind: 'leaf', name: 'id', path: 'records[2].id', type: 'u64', offset: '9007199254740993', length: '8', value: '42', endianness: 'big', comment: '', enumLabel: 'Answer', flags: [], diagnostics: [], children: [] }
    const issue: ParsedNode = { ...id, kind: 'error', name: 'missing', path: 'records[2].missing', offset: null, length: null, value: null, diagnostics: [{ code: 'template_out_of_bounds', message: 'records[2].missing: beyond file' }] }
    const root: ParsedNode = { ...id, kind: 'array', name: 'records', path: 'records', type: 'array', value: null, children: [id, issue] }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [root], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    expect(wrapper.find('[data-testid="result-tree"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('records[2].missing: beyond file')
    await wrapper.get('[data-result-path="records[2].id"]').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 9007199254740993n, end: 9007199254741000n }]])
    await wrapper.get('[data-result-path="records[2].missing"]').trigger('click')
    expect(wrapper.emitted('navigate')).toHaveLength(1)
    await wrapper.get('[data-result-path="records"]').trigger('click')
    expect(wrapper.find('[data-result-path="records[2].id"]').exists()).toBe(false)
  })
  it('shows leaf result columns in the bottom pane while preserving exact navigation', async () => {
    const result: ParsedNode = { ...leaf('magic', '16', '4', '0x1234'), type: 'u32', comment: 'Header' }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [result], collapsed: false, hasFile: true, templateSource: 'file', templateApplied: true } })
    const row = wrapper.get('[data-testid="parsed-result"]')
    expect(wrapper.get('.result-columns').text()).toBe('PathValue / DiagnosticOffsetSizeType')
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
    const fields: ParsedNode[] = [
      { ...leaf('first', '16', '4', '1'), type: 'u32' },
      leaf('second', '20', '1', '2'),
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
