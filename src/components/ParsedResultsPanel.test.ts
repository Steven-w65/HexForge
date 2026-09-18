import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ParsedField } from '../types'
import ParsedResultsPanel from './ParsedResultsPanel.vue'

describe('ParsedResultsPanel', () => {
  it('renders the required columns and emits the exact inclusive byte range', async () => {
    const result: ParsedField = { name: 'magic', offset: '16', type: 'u32', length: 4, endianness: 'little', value: '0x1234', comment: 'Header' }
    const wrapper = mount(ParsedResultsPanel, { props: { results: [result], collapsed: false } })
    expect(wrapper.findAll('th').map((cell) => cell.text())).toEqual(['Name', 'Offset', 'Type', 'Value', 'Comment'])
    await wrapper.get('tbody tr').trigger('click')
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 16n, end: 19n }]])
  })
})
