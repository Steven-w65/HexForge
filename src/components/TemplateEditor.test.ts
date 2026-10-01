import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import type { TemplateDefinition } from '../types'
import { newTemplate } from '../template/model'
import TemplateEditor from './TemplateEditor.vue'

function latest(wrapper: ReturnType<typeof mount>): TemplateDefinition {
  return wrapper.emitted('update:modelValue')!.at(-1)![0] as TemplateDefinition
}

describe('TemplateEditor', () => {
  it('renames a field atomically with its references and rejects duplicate names', async () => {
    const original: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'size', type: 'u8' }, { name: 'payload', type: 'bytes', length: { ref: 'size', max: 8 } },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: original, canApply: true } })
    await wrapper.get('[data-field-path="size"] [data-field="name"]').setValue('length')
    expect(latest(wrapper).fields[1]!.length).toEqual({ ref: 'length', max: 8 })
    await wrapper.setProps({ modelValue: latest(wrapper) })
    const emissions = wrapper.emitted('update:modelValue')!.length
    await wrapper.get('[data-field-path="length"] [data-field="name"]').setValue('payload')
    expect(wrapper.emitted('update:modelValue')).toHaveLength(emissions)
    expect(wrapper.get('[data-testid="validation-summary"]').text()).toContain('already exists')
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeDefined()
    expect(original.fields[0]!.name).toBe('size')
    wrapper.unmount()
  })

  it('offers cancellation before reordering a referenced field and refuses a stale confirmation', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'size', type: 'u8' }, { name: 'payload', type: 'bytes', length: { ref: 'size', max: 8 } },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model } })
    await wrapper.get('[data-field-path="size"] [data-action="move-field-down"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    expect(wrapper.get('[data-testid="reference-warning"]').text()).toContain('payload')
    await wrapper.get('[data-action="cancel-reference-change"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('[data-field-path="size"] [data-action="move-field-down"]').trigger('click')
    await wrapper.setProps({ modelValue: { ...model, name: 'Newer edit' } })
    expect(wrapper.find('[data-testid="reference-warning"]').exists()).toBe(false)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('[data-field-path="size"] [data-action="move-field-down"]').trigger('click')
    await wrapper.get('[data-action="confirm-reference-change"]').trigger('click')
    expect(latest(wrapper).fields.map(field => field.name)).toEqual(['payload', 'size'])
    wrapper.unmount()
  })

  it('lists dependents before removing a nested reference source', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'header', type: 'struct', fields: [{ name: 'size', type: 'u8' }] },
      { name: 'payload', type: 'bytes', length: { ref: 'header.size', max: 8 } },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model } })
    await wrapper.get('[data-field-path="header.size"] [data-action="remove-field"]').trigger('click')
    await wrapper.get('[data-action="confirm-field-change"]').trigger('click')
    expect(wrapper.get('[data-testid="reference-warning"]').text()).toContain('payload')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('[data-action="cancel-reference-change"]').trigger('click')
    expect(model.fields[0]!.fields).toHaveLength(1)
    wrapper.unmount()
  })

  it('keeps collapse state attached to a field across editing, renaming and reordering without serializing UI IDs', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'header', type: 'struct', fields: [{ name: 'id', type: 'u8' }] }, { name: 'tail', type: 'u8' },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model } })
    await wrapper.get('[data-field-path="header"] [data-action="toggle-field"]').trigger('click')
    expect(wrapper.get('[data-field-path="header"] [data-testid="field-summary"]').text()).toContain('header · struct · 1 field')
    await wrapper.get('[data-field-path="header"] [data-action="move-field-down"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.get('[data-field-path="header"] [data-action="toggle-field"]').attributes('aria-expanded')).toBe('false')
    expect(wrapper.get('[data-field-path="tail"] [data-action="toggle-field"]').attributes('aria-expanded')).toBe('true')
    await wrapper.get('[data-field-path="header"] [data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-field-path="header.id"] [data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-field-path="header"] [data-field="name"]').setValue('metadata')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.get('[data-field-path="metadata.id"] [data-action="toggle-field"]').attributes('aria-expanded')).toBe('false')
    expect(JSON.stringify(latest(wrapper))).not.toMatch(/field-\d+|expanded|editorId/)
    wrapper.unmount()
  })

  it('opens folded ancestor cards and advanced controls when an error is clicked, then focuses the invalid control', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'header', type: 'struct', fields: [{ name: 'id', type: 'u8', align: 3 }] },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model }, attachTo: document.body })
    const input = wrapper.get('[data-field-path="header.id"] [data-field="align"]')
    const scroll = vi.fn()
    input.element.scrollIntoView = scroll
    await wrapper.get('[data-field-path="header.id"] [data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-field-path="header"] [data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-testid="validation-summary"] [data-action="reveal-error"]').trigger('click')
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(wrapper.get('[data-field-path="header"] [data-action="toggle-field"]').attributes('aria-expanded')).toBe('true')
    expect(wrapper.get('[data-field-path="header.id"] [data-testid="field-advanced"]').attributes('open')).toBeDefined()
    expect(document.activeElement).toBe(input.element)
    expect(scroll).toHaveBeenCalled()
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('keeps focus and expansion on the same nested field when its siblings reorder', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'header', type: 'struct', fields: [
      { name: 'first', type: 'u8' }, { name: 'second', type: 'u8' },
    ] }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model }, attachTo: document.body })
    const input = wrapper.get('[data-field-path="header.first"] [data-field="name"]').element as HTMLInputElement
    input.focus()
    await wrapper.get('[data-field-path="header.first"] [data-action="move-field-down"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.get('[data-field-path="header.first"] [data-field="name"]').element).toBe(input)
    expect(document.activeElement).toBe(input)
    wrapper.unmount()
  })

  it('gates Save and Apply on duplicate enum edits until the conflicting input is corrected', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'kind', type: 'u8', enumLabels: { '1': 'One', '2': 'Two' } }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model, canApply: true, canSave: true, canSaveAs: true } })
    const input = wrapper.findAll('[data-field="enum-key"]')[0]!
    await input.setValue('2')
    expect(wrapper.get('[data-testid="validation-summary"]').text()).toContain('already exists')
    for (const action of ['save-template', 'save-template-as', 'apply-template']) expect(wrapper.get(`[data-action="${action}"]`).attributes('disabled')).toBeDefined()
    await input.setValue('3')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    for (const action of ['save-template', 'save-template-as', 'apply-template']) expect(wrapper.get(`[data-action="${action}"]`).attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('clears a rejected enum key when a confirmed type change removes enum configuration', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'kind', type: 'u8', enumLabels: { '1': 'One', '2': 'Two' } }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model, canApply: true, canSave: true } })
    await wrapper.findAll('[data-field="enum-key"]')[0]!.setValue('2')
    expect(wrapper.get('[data-testid="validation-summary"]').text()).toContain('already exists')
    await wrapper.get('[data-field="type"]').setValue('bool')
    await wrapper.get('[data-action="confirm-field-change"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.find('[data-testid="validation-summary"]').exists()).toBe(false)
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('navigates to a field literally named template rather than the template-name input', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'template', type: 'u8', align: 3 }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model }, attachTo: document.body })
    const input = wrapper.get('[data-field-path="template"] [data-field="align"]')
    await wrapper.get('[data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-testid="validation-summary"] [data-action="reveal-error"]').trigger('click')
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(wrapper.get('[data-action="toggle-field"]').attributes('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(input.element)
    wrapper.unmount()
  })

  it.each([
    { label: 'blank enum label', field: { name: 'kind', type: 'u8', enumLabels: { '1': 'One', '2': '' } }, control: 'enum-label' },
    { label: 'out-of-range enum key', field: { name: 'kind', type: 'u8', enumLabels: { '1': 'One', '256': 'Too large' } }, control: 'enum-key' },
    { label: 'blank flag name', field: { name: 'kind', type: 'u8', bitFlags: [{ bit: 0, name: 'first' }, { bit: 1, name: '' }] }, control: 'flag-name' },
    { label: 'duplicate flag bit', field: { name: 'kind', type: 'u8', bitFlags: [{ bit: 0, name: 'first' }, { bit: 0, name: 'second' }] }, control: 'flag-bit' },
  ] as const)('focuses the affected second metadata row for $label', async ({ field, control }) => {
    const model = { ...newTemplate(), fields: [JSON.parse(JSON.stringify(field))] } as TemplateDefinition
    const wrapper = mount(TemplateEditor, { props: { modelValue: model }, attachTo: document.body })
    const input = wrapper.findAll(`[data-field="${control}"]`)[1]!
    await wrapper.get('[data-action="toggle-field"]').trigger('click')
    await wrapper.get('[data-testid="validation-summary"] [data-action="reveal-error"]').trigger('click')
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(document.activeElement).toBe(input.element)
    expect((input.element.closest('details') as HTMLDetailsElement).open).toBe(true)
    wrapper.unmount()
  })
  it('edits a bounded length reference and a checked offset reference through the field controls', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'size', type: 'u16' }, { name: 'offset', type: 'u64' },
      { name: 'payload', type: 'bytes', length: 2, placement: { mode: 'absolute', offset: '0' } },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model, canApply: true } })
    const set = async (selector: string, value: string) => {
      await wrapper.get(`[data-field-path="payload"] ${selector}`).setValue(value)
      await wrapper.setProps({ modelValue: latest(wrapper) })
    }
    await set('[data-field="length-source"]', 'ref')
    await set('[data-field="length-ref"]', 'size')
    await set('[data-field="length-max"]', '256')
    await set('[data-field="offset-source"]', 'ref')
    await set('[data-field="offset-ref"]', 'offset')
    await set('[data-field="offset-add"]', '2')
    expect(latest(wrapper).fields[2]).toMatchObject({ length: { ref: 'size', max: 256 }, placement: { mode: 'absolute', offset: { ref: 'offset', add: '2' } } })
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeUndefined()
    expect(model.fields[2]?.length).toBe(2)
  })

  it('edits expected values and rejects an inverted range before Apply', async () => {
    const wrapper = mount(TemplateEditor, { props: { modelValue: { ...newTemplate(), fields: [{ name: 'size', type: 'u16' }] }, canApply: true } })
    await wrapper.get('[data-action="expect-toggle"]').setValue(true)
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-field="expect-kind"]').setValue('range')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-field="expect-min"]').setValue('5')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-field="expect-max"]').setValue('1')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.get('[data-testid="field-error"]').text()).toContain('minimum')
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-field="expect-max"]').setValue('10')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(latest(wrapper).fields[0]).toMatchObject({ expect: { kind: 'range', min: { type: 'unsigned', value: '5' }, max: { type: 'unsigned', value: '10' } } })
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeUndefined()
  })

  it('creates and reorders nested structures and arrays without mutating the original model', async () => {
    const original = newTemplate()
    const wrapper = mount(TemplateEditor, { props: { modelValue: original } })
    await wrapper.get('[data-action="add-field"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-field="type"]').setValue('struct')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-action="add-child-field"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(latest(wrapper).fields[0]).toMatchObject({ type: 'struct', fields: [{ name: 'field1', type: 'u8' }] })
    await wrapper.get('[data-action="add-field"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.findAll('[data-field="type"]')[2]!.setValue('array')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(latest(wrapper).fields[1]).toMatchObject({ type: 'array', count: { fixed: 1 }, element: { type: 'u8' } })
    await wrapper.get('[data-field-path="field2"] [data-action="move-field-up"]').trigger('click')
    expect(latest(wrapper).fields.map(field => field.type)).toEqual(['array', 'struct'])
    expect(original.fields).toEqual([])
  })

  it('edits references and conditions and reports invalid drafts beside the field', async () => {
    const model: TemplateDefinition = { ...newTemplate(), fields: [
      { name: 'count', type: 'u8' },
      { name: 'records', type: 'array', count: { ref: 'missing' }, element: { type: 'u8' } },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model, canApply: true } })
    expect(wrapper.get('[data-testid="field-error"]').text()).toContain('earlier field')
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-field="count-ref"]').setValue('count')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(wrapper.find('[data-testid="field-error"]').exists()).toBe(false)
    await wrapper.findAll('[data-action="condition-toggle"]')[1]!.setValue(true)
    await wrapper.setProps({ modelValue: latest(wrapper) })
    await wrapper.get('[data-field="condition-ref"]').setValue('count')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(latest(wrapper).fields[1]?.condition).toMatchObject({ ref: 'count', op: 'eq', value: { type: 'unsigned', value: '0' } })
    expect(wrapper.get('[data-action="apply-template"]').attributes('disabled')).toBeUndefined()
  })

  it('navigates a parsed leaf with a lossless bigint offset from the separate editor', async () => {
    const offset = '9007199254740993'
    const wrapper = mount(TemplateEditor, { props: {
      modelValue: { ...newTemplate(), fields: [{ name: 'id', type: 'u64' }] },
      results: [{ kind: 'leaf', name: 'id', path: 'id', type: 'u64', offset, length: '8', value: offset,
        endianness: 'little', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] }],
    } })
    await wrapper.get('[data-action="navigate-field"]').trigger('click')
    expect(wrapper.emitted('navigate')?.[0]).toEqual([{ start: 9007199254740993n, end: 9007199254741000n }])
  })

  it('duplicates a top-level field and a nested structure field in their own scopes', async () => {
    const model: TemplateDefinition = { name: 'Records', defaultEndianness: 'little', fields: [
      { name: 'record', type: 'struct', fields: [{ name: 'id', type: 'u8' }] },
    ] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: model } })
    await wrapper.get('[data-field-path="record"] [data-action="duplicate-field"]').trigger('click')
    await wrapper.setProps({ modelValue: latest(wrapper) })
    expect(latest(wrapper).fields.map(field => field.name)).toEqual(['record', 'recordCopy'])
    await wrapper.get('[data-field-path="record.id"] [data-action="duplicate-field"]').trigger('click')
    expect(latest(wrapper).fields[0]?.fields?.map(field => field.name)).toEqual(['id', 'idCopy'])
    expect(model.fields[0]?.fields?.map(field => field.name)).toEqual(['id'])
  })
})
