import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { TemplateDefinition } from '../types'
import { newTemplate } from '../template/model'
import TemplateEditor from './TemplateEditor.vue'

function latest(wrapper: ReturnType<typeof mount>): TemplateDefinition {
  return wrapper.emitted('update:modelValue')!.at(-1)![0] as TemplateDefinition
}

describe('TemplateEditor', () => {
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
