import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { TemplateDefinition } from '../types'
import TemplateEditor from './TemplateEditor.vue'

const emptyTemplate = (): TemplateDefinition => ({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] })

describe('TemplateEditor', () => {
  it('adds a valid numeric field and reveals length only for variable-width types', async () => {
    const wrapper = mount(TemplateEditor, { props: { modelValue: emptyTemplate() } })
    await wrapper.get('[data-action="add-field"]').trigger('click')
    const template = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
    expect(template.fields[0]).toEqual({ name: 'field1', offset: '0', type: 'u8', endianness: 'little', comment: '' })
    expect(wrapper.find('input[data-field="length"]').exists()).toBe(false)
    await wrapper.setProps({ modelValue: { ...template, fields: [{ ...template.fields[0]!, type: 'bytes', length: 4 }] } })
    expect(wrapper.find('input[data-field="length"]').exists()).toBe(true)
  })

  it('initializes a positive serialized length when the type changes to string or bytes', async () => {
    const original = { ...emptyTemplate(), fields: [{ name: 'data', offset: '0', type: 'u16' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: original } })
    await wrapper.get('select[data-field="type"]').setValue('string')
    const updated = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
    expect(updated.fields[0]).toEqual({ ...original.fields[0], type: 'string', length: 1 })
    expect(original.fields[0]).not.toHaveProperty('length')
  })

  it('emits immutable field updates without serializing UI row ids', async () => {
    const original = { ...emptyTemplate(), fields: [{ name: 'a', offset: '4', type: 'u16' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: original } })
    await wrapper.get('input[data-field="name"]').setValue('header')
    const updated = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
    expect(original.fields[0]!.name).toBe('a')
    expect(updated.fields[0]!.name).toBe('header')
    expect(Object.keys(updated.fields[0]!)).not.toContain('id')
  })

  it('offers every supported type and emits save, load, and exact field navigation', async () => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', offset: '16', type: 'u32' as const, endianness: 'big' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    expect(wrapper.findAll('select[data-field="type"] option').map((option) => option.attributes('value'))).toEqual([
      'u8', 'u16', 'u32', 'i8', 'i16', 'i32', 'f32', 'f64', 'string', 'bytes',
    ])
    await wrapper.get('[data-action="save-template"]').trigger('click')
    await wrapper.get('[data-action="load-template"]').trigger('click')
    await wrapper.get('[data-action="navigate-field"]').trigger('click')
    expect(wrapper.emitted('save')).toHaveLength(1)
    expect(wrapper.emitted('load')).toHaveLength(1)
    expect(wrapper.emitted('navigate')).toEqual([[{ start: 16n, end: 19n }]])
  })

  it.each([
    ['an in-progress offset', { offset: 'x', length: 4 }],
    ['a fractional length', { offset: '16', length: 1.5 }],
    ['a non-finite length', { offset: '16', length: Number.NaN }],
    ['a non-positive length', { offset: '16', length: 0 }],
  ])('does not throw or navigate for %s', async (_label, invalid) => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', type: 'bytes' as const, endianness: 'little' as const, comment: '', ...invalid }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    await expect(wrapper.get('[data-action="navigate-field"]').trigger('click')).resolves.toBeUndefined()
    expect(wrapper.emitted('navigate')).toBeUndefined()
  })

  it('does not emit backend-incompatible lengths entered in the editor', async () => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', offset: '16', type: 'bytes' as const, length: 4, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    await wrapper.get('input[data-field="length"]').setValue('1.5')
    const input = wrapper.get<HTMLInputElement>('input[data-field="length"]')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    expect(input.element.value).toBe('4')
    expect(input.attributes('aria-invalid')).toBe('true')
    expect(wrapper.get('[role="alert"]').text()).toContain('positive whole number')
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeDefined()
  })

  it('restores an invalid partial offset while clearly marking the rejected draft', async () => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', offset: '16', type: 'u8' as const, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    const input = wrapper.get<HTMLInputElement>('input[data-field="offset"]')
    await input.setValue('0x')
    expect(input.element.value).toBe('16')
    expect(input.attributes('aria-invalid')).toBe('true')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await input.setValue('0x20')
    expect(input.attributes('aria-invalid')).toBeUndefined()
  })

  it('rejects offsets beyond u64 and lengths beyond the backend field limit', async () => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', offset: '16', type: 'bytes' as const, length: 4, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    const offset = wrapper.get<HTMLInputElement>('input[data-field="offset"]')
    const length = wrapper.get<HTMLInputElement>('input[data-field="length"]')
    await offset.setValue('18446744073709551616')
    await length.setValue('1048577')
    expect(offset.element.value).toBe('16')
    expect(length.element.value).toBe('4')
    expect(wrapper.findAll('[aria-invalid="true"]')).toHaveLength(2)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('clears a stale length error when switching to a fixed-width type', async () => {
    const value = { ...emptyTemplate(), fields: [{ name: 'data', offset: '16', type: 'bytes' as const, length: 4, endianness: 'little' as const, comment: '' }] }
    const wrapper = mount(TemplateEditor, { props: { modelValue: value } })
    await wrapper.get('input[data-field="length"]').setValue('0')
    await wrapper.get('select[data-field="type"]').setValue('u8')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.get('[data-action="save-template"]').attributes('disabled')).toBeUndefined()
  })

  it('normalizes a hexadecimal offset and inherits the template default endianness', async () => {
    const wrapper = mount(TemplateEditor, { props: { modelValue: { ...emptyTemplate(), defaultEndianness: 'big' } } })
    await wrapper.get('[data-action="add-field"]').trigger('click')
    let template = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
    expect(template.fields[0]?.endianness).toBe('big')
    await wrapper.setProps({ modelValue: template })
    await wrapper.get('input[data-field="offset"]').setValue('0x2A')
    template = wrapper.emitted('update:modelValue')?.at(-1)?.[0] as TemplateDefinition
    expect(template.fields[0]?.offset).toBe('42')
  })
})
