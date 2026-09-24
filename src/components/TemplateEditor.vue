<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import type { FieldType, ParsedField, TemplateDefinition, TemplateField } from '../types'

const props = withDefaults(defineProps<{
  modelValue: TemplateDefinition
  fileSize?: bigint | null
  results?: ParsedField[]
  canApply?: boolean
}>(), { fileSize: null, results: () => [], canApply: false })
const emit = defineEmits<{
  'update:modelValue': [value: TemplateDefinition]
  save: []; load: []; apply: []; navigate: [range: { start: bigint; end: bigint }]; validity: [valid: boolean]
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
  return { name: `field${props.modelValue.fields.length + 1}`, offset: nextAvailableOffset(), type: 'u8', endianness: props.modelValue.defaultEndianness, comment: '' }
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

function duplicateField(index: number): void {
  const source = props.modelValue.fields[index]
  if (!source) return
  rowIds.value.push(nextId++)
  updateTemplate({ fields: [...props.modelValue.fields, { ...source, name: `${source.name} copy`, offset: nextAvailableOffset() }] })
  publishValidity()
}

function moveField(index: number, delta: -1 | 1): void {
  const destination = index + delta
  if (destination < 0 || destination >= props.modelValue.fields.length) return
  const fields = [...props.modelValue.fields]
  ;[fields[index], fields[destination]] = [fields[destination]!, fields[index]!]
  const ids = [...rowIds.value]
  ;[ids[index], ids[destination]] = [ids[destination]!, ids[index]!]
  rowIds.value = ids
  updateTemplate({ fields })
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

function nextAvailableOffset(): string {
  let next = 0n
  for (const field of props.modelValue.fields) {
    const range = fieldRange(field)
    if (range && range.end + 1n > next) next = range.end + 1n
  }
  return next.toString()
}

function formatOffset(offset: bigint): string {
  return `0x${offset.toString(16).toUpperCase().padStart(8, '0')}`
}

function rangeLabel(field: TemplateField): string {
  const range = fieldRange(field)
  if (!range) return 'Invalid byte range'
  return `${formatOffset(range.start)}–${formatOffset(range.end)} · ${(range.end - range.start + 1n).toString()} bytes`
}

function isOutsideFile(field: TemplateField): boolean {
  const range = fieldRange(field)
  return props.fileSize !== null && range !== null && range.end >= props.fileSize
}

function parsedValue(field: TemplateField, index: number): string | null {
  const direct = props.results[index]
  if (direct) return direct.value
  return props.results.find((result) => result.name === field.name && result.offset === field.offset)?.value ?? null
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
          <div class="field-tools">
            <button type="button" data-action="navigate-field" title="Show bytes" aria-label="Show field bytes" :disabled="fieldRange(field) === null" @click="navigate(field)">↗</button>
            <button type="button" data-action="move-field-up" title="Move field up" aria-label="Move field up" :disabled="index === 0" @click="moveField(index, -1)">↑</button>
            <button type="button" data-action="move-field-down" title="Move field down" aria-label="Move field down" :disabled="index === modelValue.fields.length - 1" @click="moveField(index, 1)">↓</button>
            <button type="button" data-action="duplicate-field" title="Duplicate field" aria-label="Duplicate field" @click="duplicateField(index)">⧉</button>
            <button type="button" title="Remove field" aria-label="Remove field" @click="removeField(index)">×</button>
          </div>
        </div>
        <div class="field-grid">
          <label>Offset<input data-field="offset" :value="field.offset" :aria-invalid="validationError(index, 'offset') ? 'true' : undefined" @input="updateOffset(index, ($event.target as HTMLInputElement).value, $event.target as HTMLInputElement)"></label>
          <label>Type<select data-field="type" :value="field.type" @change="updateField(index, { type: ($event.target as HTMLSelectElement).value as FieldType })"><option v-for="type in types" :key="type" :value="type">{{ type }}</option></select></label>
          <label v-if="field.type === 'string' || field.type === 'bytes'">Length<input data-field="length" type="number" min="1" step="1" :value="field.length ?? 1" :aria-invalid="validationError(index, 'length') ? 'true' : undefined" @input="updateLength(index, ($event.target as HTMLInputElement).value, $event.target as HTMLInputElement)"></label>
          <label>Endian<select data-field="endianness" :value="field.endianness ?? modelValue.defaultEndianness" @change="updateField(index, { endianness: ($event.target as HTMLSelectElement).value as 'little' | 'big' })"><option value="little">LE</option><option value="big">BE</option></select></label>
        </div>
        <div class="field-summary">
          <span data-testid="field-range">{{ rangeLabel(field) }}</span>
          <strong v-if="parsedValue(field, index) !== null" data-testid="field-preview">{{ parsedValue(field, index) }}</strong>
        </div>
        <p v-if="isOutsideFile(field)" data-testid="range-warning" class="range-warning">This range is outside the current file.</p>
        <input data-field="comment" aria-label="Comment" placeholder="Comment" :value="field.comment" @input="updateField(index, { comment: ($event.target as HTMLInputElement).value })">
        <p v-if="validationError(index, 'offset') || validationError(index, 'length')" role="alert" class="validation-error">{{ validationError(index, 'offset') ?? validationError(index, 'length') }}</p>
      </article>
      <p v-if="modelValue.fields.length === 0" class="empty">No fields defined.</p>
    </div>
    <div class="template-actions">
      <div><button type="button" data-action="add-field" @click="addField">+ Field</button>
        <button type="button" data-action="load-template" @click="emit('load')">Load…</button>
        <button type="button" data-action="save-template" :disabled="Object.keys(validationErrors).length > 0" @click="emit('save')">Save As…</button>
      </div>
      <button type="button" class="primary" data-action="apply-template" :disabled="!canApply || Object.keys(validationErrors).length > 0" @click="emit('apply')">Apply Template</button>
    </div>
  </section>
</template>

<style scoped>
.template-editor { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 11px; }
.template-meta { display: grid; grid-template-columns: minmax(0, 1fr) 150px; gap: 9px; padding-bottom: 2px; }
.field-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 7px; }
label { display: grid; gap: 3px; color: var(--muted); font-size: var(--font-support); }
input, select { box-sizing: border-box; min-width: 0; width: 100%; height: 28px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 4px; outline: none; font: inherit; font-size: var(--font-body); }
input:focus, select:focus { border-color: var(--selection); box-shadow: 0 0 0 1px color-mix(in srgb, var(--selection) 48%, transparent); }
input[aria-invalid='true'] { border-color: var(--modified); }
.field-list { flex: 1; min-height: 0; padding-right: 3px; overflow: auto; }
.field-card { display: grid; gap: 8px; margin-bottom: 8px; padding: 9px; background: var(--card); border: 1px solid var(--border); border-radius: 5px; }
.field-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; }
.field-tools { display: flex; gap: 3px; }
.field-tools button { width: 24px; height: 28px; }
button { color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); }
button:hover:not(:disabled) { background: var(--hover); }
button:disabled { opacity: .4; }
.empty { color: var(--muted); font-size: var(--font-body); }
.validation-error { margin: 0; color: var(--modified); font-size: var(--font-support); }
.field-summary { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-width: 0; color: var(--muted); font-size: var(--font-support); }
.field-summary strong { overflow: hidden; color: var(--text); font-size: var(--font-body); text-overflow: ellipsis; white-space: nowrap; }
.range-warning { margin: -2px 0 0; color: var(--modified); font-size: var(--font-support); }
.template-actions { display: flex; justify-content: space-between; gap: 8px; padding-top: 10px; border-top: 1px solid var(--border); }
.template-actions > div { display: flex; gap: 6px; }
.template-actions button { height: 27px; padding: 0 9px; }
.template-actions .primary { color: var(--selection-text); background: var(--selection); }
.template-actions .primary:hover:not(:disabled) { filter: brightness(1.12); }
</style>
