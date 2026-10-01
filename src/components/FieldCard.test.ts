import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { TemplateField } from '../types'
import FieldCard from './FieldCard.vue'

const props = (field: TemplateField) => ({ field, path: field.name ?? 'record', issues: [], preview: [] })

describe('safe field editing', () => {
  it('rejects a duplicate enum key without overwriting either label and recovers after correction', async () => {
    const field: TemplateField = { name: 'kind', type: 'u8', enumLabels: { '1': 'One', '2': 'Two' } }
    const wrapper = mount(FieldCard, { props: props(field) })
    const input = wrapper.findAll('[aria-label="Enum value"]')[0]!
    await input.setValue('2')
    expect(wrapper.emitted('update:field')).toBeUndefined()
    expect(wrapper.get('[data-testid="enum-key-error"]').text()).toContain('already exists')
    expect(field.enumLabels).toEqual({ '1': 'One', '2': 'Two' })
    await input.setValue('3')
    expect(wrapper.emitted('update:field')?.[0]?.[0]).toMatchObject({ enumLabels: { '3': 'One', '2': 'Two' } })
    expect(wrapper.find('[data-testid="enum-key-error"]').exists()).toBe(false)
    wrapper.unmount()
  })
  it('keeps nested data until a destructive type change is explicitly confirmed', async () => {
    const original: TemplateField = { name: 'record', type: 'struct', fields: [{ name: 'id', type: 'u64' }] }
    const wrapper = mount(FieldCard, { props: props(original) })
    await wrapper.get('[data-field="type"]').setValue('u8')
    expect(wrapper.emitted('update:field')).toBeUndefined()
    expect(wrapper.get('[role="dialog"]').text()).toContain('record')
    await wrapper.get('[data-action="cancel-field-change"]').trigger('click')
    expect(wrapper.emitted('update:field')).toBeUndefined()
    await wrapper.get('[data-field="type"]').setValue('u8')
    await wrapper.get('[data-action="confirm-field-change"]').trigger('click')
    expect(wrapper.emitted('update:field')?.[0]?.[0]).toEqual({ name: 'record', type: 'u8' })
    expect(original.fields?.[0]?.type).toBe('u64')
    wrapper.unmount()
  })

  it('requires confirmation to remove a configured field but not an empty new field', async () => {
    const wrapper = mount(FieldCard, { props: props({ name: 'field1', type: 'u8' }) })
    await wrapper.get('[data-action="remove-field"]').trigger('click')
    expect(wrapper.emitted('remove')).toHaveLength(1)
    await wrapper.setProps({ field: { name: 'record', type: 'struct', fields: [{ name: 'id', type: 'u8' }] } })
    await wrapper.get('[data-action="remove-field"]').trigger('click')
    expect(wrapper.emitted('remove')).toHaveLength(1)
    await wrapper.get('[data-action="confirm-field-change"]').trigger('click')
    expect(wrapper.emitted('remove')).toHaveLength(2)
    wrapper.unmount()
  })

  it('does not discard a newer edit that arrives while confirmation is open', async () => {
    const wrapper = mount(FieldCard, { props: props({ name: 'record', type: 'struct', fields: [{ name: 'id', type: 'u8' }] }) })
    await wrapper.get('[data-field="type"]').setValue('u8')
    await wrapper.setProps({ field: { name: 'record', type: 'struct', fields: [{ name: 'id', type: 'u64' }] } })
    if (wrapper.find('[data-action="confirm-field-change"]').exists()) await wrapper.get('[data-action="confirm-field-change"]').trigger('click')
    expect(wrapper.emitted('update:field')).toBeUndefined()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('collapses advanced controls by default and summarizes configured rules', () => {
    const wrapper = mount(FieldCard, { props: props({ name: 'id', type: 'u8', align: 4, comment: 'Identifier', expect: { kind: 'equals', value: { type: 'unsigned', value: '7' } } }) })
    const advanced = wrapper.get('[data-testid="field-advanced"]')
    expect(advanced.attributes('open')).toBeUndefined()
    expect(advanced.get('summary').text()).toContain('Expected: 7')
    expect(advanced.get('summary').text()).toContain('Align 4')
    expect(wrapper.get('[data-field="placement-mode"]').element.closest('details')).toBeNull()
    expect(wrapper.get('[data-field="align"]').element.closest('details')).toBe(advanced.element)
    wrapper.unmount()
  })
})
