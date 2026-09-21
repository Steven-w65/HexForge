<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import type { FieldType, TemplateDefinition, TemplateField } from '../types'

const props = defineProps<{ modelValue: TemplateDefinition }>()
const emit = defineEmits<{
  'update:modelValue': [value: TemplateDefinition]
  save: []; load: []; navigate: [range: { start: bigint; end: bigint }]; validity: [valid: boolean]
}>()

const types: FieldType[] = ['u8', 'u16', 'u32', 'i8', 'i16', 'i32', 'f32', 'f64', 'string', 'bytes']
const offsetLiteral = /^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/
const MAX_U64 = (1n << 64n) - 1n
const MAX_FIELD_LENGTH = 1024 * 1024
let nextId = 1
const rowIds = ref(props.modelValue.fields.map(() => nextId++))
const validationErrors = reactive<Record<number, { offset?: string; length?: string }>>({})
let lastEmittedModel: TemplateDefinition | null = null
watch(() => props.modelValue.fields.length, (length) => {
  while (rowIds.value.length < length) rowIds.value.push(nextId++)
  rowIds.value.length = length
})
watch(() => props.modelValue, (value) => {
  if (value === lastEmittedModel) { lastEmittedModel = null; return }
  for (const key of Object.keys(validationErrors)) delete validationErrors[Number(key)]
  publishValidity()
})

function defaultField(): TemplateField {
  return { name: `field${props.modelValue.fields.length + 1}`, offset: '0', type: 'u8', endianness: props.modelValue.defaultEndianness, comment: '' }
}

function setValidationError(index: number, key: 'offset' | 'length', message?: string): void {
  const id = rowIds.value[index]!
  if (message) validationErrors[id] = { ...validationErrors[id], [key]: message }
  else {
    const next = { ...validationErrors[id] }
    delete next[key]
    if (Object.keys(next).length) validationErrors[id] = next
    else delete validationErrors[id]
  }
  publishValidity()
}

function publishValidity(): void { emit('validity', Object.keys(validationErrors).length === 0) }

function validationError(index: number, key: 'offset' | 'length'): string | undefined {
  return validationErrors[rowIds.value[index]!]?.[key]
}

function updateOffset(index: number, text: string, input: HTMLInputElement): void {
  const value = text.trim()
  if (!offsetLiteral.test(value)) {
    input.value = props.modelValue.fields[index]!.offset
    setValidationError(index, 'offset', 'Enter a decimal or 0x-prefixed hexadecimal offset.')
    return
  }
  try {
    const offset = BigInt(value)
    if (offset > MAX_U64) throw new RangeError('offset exceeds u64')
    updateField(index, { offset: offset.toString() })
    setValidationError(index, 'offset')
  } catch {
    input.value = props.modelValue.fields[index]!.offset
    setValidationError(index, 'offset', 'The offset exceeds the supported range.')
  }
}

function updateTemplate(patch: Partial<TemplateDefinition>): void {
  const next = { ...props.modelValue, ...patch }
  lastEmittedModel = next
  emit('update:modelValue', next)
}

function updateField(index: number, patch: Partial<TemplateField>): void {
  const fields = props.modelValue.fields.map((field, current) => current === index ? { ...field, ...patch } : field)
  const updated = fields[index]!
  if (isVariableWidth(updated.type)) {
    if (parseLength(updated.length) === null) updated.length = 1
  } else {
    delete updated.length
    setValidationError(index, 'length')
  }
  updateTemplate({ fields })
}

function updateLength(index: number, text: string, input: HTMLInputElement): void {
  const length = parseLength(Number(text))
  if (length === null) {
    input.value = String(props.modelValue.fields[index]!.length ?? 1)
    setValidationError(index, 'length', 'Length must be a positive whole number no greater than 1048576.')
    return
  }
  updateField(index, { length })
  setValidationError(index, 'length')
}

function addField(): void {
  rowIds.value.push(nextId++)
  updateTemplate({ fields: [...props.modelValue.fields, defaultField()] })
  publishValidity()
}

function removeField(index: number): void {
  delete validationErrors[rowIds.value[index]!]
  rowIds.value.splice(index, 1)
  updateTemplate({ fields: props.modelValue.fields.filter((_, current) => current !== index) })
  publishValidity()
}

function isVariableWidth(type: FieldType): boolean {
  return type === 'string' || type === 'bytes'
}

function parseLength(value: unknown): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 && value <= MAX_FIELD_LENGTH ? value : null
}

function fieldLength(field: TemplateField): number | null {
  if (isVariableWidth(field.type)) return parseLength(field.length)
  if (field.type.endsWith('8')) return 1
  if (field.type.endsWith('16')) return 2
  if (field.type.endsWith('32') || field.type === 'f32') return 4
  return 8
}

function fieldRange(field: TemplateField): { start: bigint; end: bigint } | null {
  const offset = field.offset.trim()
  const length = fieldLength(field)
  if (!offsetLiteral.test(offset) || length === null) return null
  try {
    const start = BigInt(offset)
    return { start, end: start + BigInt(length) - 1n }
  } catch {
    return null
  }
}

function navigate(field: TemplateField): void {
  const range = fieldRange(field)
  if (range) emit('navigate', range)
}
</script>

<template>
  <section class="template-editor">
    <div class="template-meta">
      <label>Name<input :value="modelValue.name" @input="updateTemplate({ name: ($event.target as HTMLInputElement).value })"></label>
      <label>Default endian<select :value="modelValue.defaultEndianness" @change="updateTemplate({ defaultEndianness: ($event.target as HTMLSelectElement).value as 'little' | 'big' })"><option value="little">Little</option><option value="big">Big</option></select></label>
    </div>
    <div class="field-list">
      <article v-for="(field, index) in modelValue.fields" :key="rowIds[index]" class="field-card">
        <div class="field-row">
          <input data-field="name" aria-label="Field name" :value="field.name" @input="updateField(index, { name: ($event.target as HTMLInputElement).value })">
          <button type="button" data-action="navigate-field" title="Show bytes" :disabled="fieldRange(field) === null" @click="navigate(field)">↗</button>
          <button type="button" title="Remove field" @click="removeField(index)">×</button>
        </div>
        <div class="field-grid">
          <label>Offset<input data-field="offset" :value="field.offset" :aria-invalid="validationError(index, 'offset') ? 'true' : undefined" @input="updateOffset(index, ($event.target as HTMLInputElement).value, $event.target as HTMLInputElement)"></label>
          <label>Type<select data-field="type" :value="field.type" @change="updateField(index, { type: ($event.target as HTMLSelectElement).value as FieldType })"><option v-for="type in types" :key="type" :value="type">{{ type }}</option></select></label>
          <label v-if="field.type === 'string' || field.type === 'bytes'">Length<input data-field="length" type="number" min="1" step="1" :value="field.length ?? 1" :aria-invalid="validationError(index, 'length') ? 'true' : undefined" @input="updateLength(index, ($event.target as HTMLInputElement).value, $event.target as HTMLInputElement)"></label>
          <label>Endian<select data-field="endianness" :value="field.endianness ?? modelValue.defaultEndianness" @change="updateField(index, { endianness: ($event.target as HTMLSelectElement).value as 'little' | 'big' })"><option value="little">LE</option><option value="big">BE</option></select></label>
        </div>
        <input data-field="comment" aria-label="Comment" placeholder="Comment" :value="field.comment" @input="updateField(index, { comment: ($event.target as HTMLInputElement).value })">
        <p v-if="validationError(index, 'offset') || validationError(index, 'length')" role="alert" class="validation-error">{{ validationError(index, 'offset') ?? validationError(index, 'length') }}</p>
      </article>
      <p v-if="modelValue.fields.length === 0" class="empty">No fields defined.</p>
    </div>
    <div class="template-actions">
      <button type="button" data-action="add-field" @click="addField">+ Field</button>
      <button type="button" data-action="load-template" @click="emit('load')">Load</button>
      <button type="button" data-action="save-template" :disabled="Object.keys(validationErrors).length > 0" @click="emit('save')">Save</button>
    </div>
  </section>
</template>

<style scoped>
.template-editor { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 9px; }
.template-meta, .field-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 7px; }
label { display: grid; gap: 3px; color: var(--muted); font-size: 10px; }
input, select { box-sizing: border-box; min-width: 0; width: 100%; height: 26px; padding: 0 6px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 3px; font: inherit; font-size: 11px; }
.field-list { flex: 1; min-height: 0; overflow: auto; }
.field-card { display: grid; gap: 7px; padding: 8px 0; border-top: 1px solid var(--border); }
.field-row { display: grid; grid-template-columns: 1fr 24px 24px; gap: 4px; }
button { color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: 11px; }
button:hover { background: var(--hover); }
.empty { color: var(--muted); font-size: 11px; }
.validation-error { margin: 0; color: var(--modified); font-size: 10px; }
.template-actions { display: flex; gap: 6px; }
.template-actions button { height: 27px; padding: 0 9px; }
</style>
