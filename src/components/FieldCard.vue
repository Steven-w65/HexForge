<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import type { Endian, NavigableParsedLeaf, ConditionValue, FieldType, TemplateDefinition, TemplateField } from '../types'
import { duplicateTemplateField, newTemplateField, type TemplateIssue } from '../template/model'
import ExpectationEditor from './ExpectationEditor.vue'
import AppDialog from './AppDialog.vue'
import { referenceCandidates } from '../template/references'

defineOptions({ name: 'FieldCard' })
const props = withDefaults(defineProps<{
  field: TemplateField
  definition?: TemplateDefinition
  path: string
  index?: number
  siblingCount?: number
  depth?: number
  isElement?: boolean
  issues: TemplateIssue[]
  preview: NavigableParsedLeaf[]
}>(), { index: 0, siblingCount: 1, depth: 0, isElement: false })
const emit = defineEmits<{
  'update:field': [field: TemplateField]
  remove: []
  duplicate: []
  move: [delta: -1 | 1]
  navigate: [path: string]
}>()

const choices: FieldType[] = ['u8', 'u16', 'u32', 'u64', 'i8', 'i16', 'i32', 'i64', 'f32', 'f64', 'bool', 'string', 'bytes', 'struct', 'array']
const integer = computed(() => /^([ui])(8|16|32|64)$/.test(props.field.type))
const unsigned = computed(() => /^u(8|16|32|64)$/.test(props.field.type))
const hasEndian = computed(() => integer.value || props.field.type === 'f32' || props.field.type === 'f64' || props.field.type === 'struct' || props.field.type === 'array')
const countMode = computed(() => props.field.count?.ref === undefined ? 'fixed' : 'ref')
const textMode = computed(() => props.field.maxLength === undefined ? 'fixed' : 'nullTerminated')
const fieldIssues = computed(() => props.issues.filter(issue => issue.path === props.path))
const matchedPreview = computed(() => props.preview.find(result => result.path === props.path))
const lengthReference = computed(() => typeof props.field.length === 'object' ? props.field.length : undefined)
const offsetReference = computed(() => typeof props.field.placement?.offset === 'object' ? props.field.placement.offset : undefined)
const pending = ref<{ kind: 'type' | 'remove'; next?: TemplateField; baseline: string } | null>(null)
const advanced = ref(false)
const listId = useId()
const unsignedRefs = computed(() => props.definition ? referenceCandidates(props.definition, props.path, 'unsigned') : [])
const conditionRefs = computed(() => props.definition ? referenceCandidates(props.definition, props.path, props.field.condition?.value.type ?? 'unsigned') : [])
const advancedSummary = computed(() => {
  const values: string[] = []
  if (props.field.align) values.push(`Align ${props.field.align}`)
  if (props.field.condition) values.push('Condition configured')
  const check = props.field.expect
  if (check) values.push(check.kind === 'equals' ? `Expected: ${String(check.value.value).slice(0, 40)}` : check.kind === 'oneOf' ? `${check.values.length} allowed values` : 'Expected range')
  if (props.field.enumLabels) values.push(`${Object.keys(props.field.enumLabels).length} labels`)
  if (props.field.bitFlags) values.push(`${props.field.bitFlags.length} flags`)
  if (props.field.comment) values.push('Comment')
  return values.join(' · ')
})
watch(() => JSON.stringify(props.field), value => { if (pending.value && value !== pending.value.baseline) pending.value = null })

function hasTypeConfiguration(): boolean {
  if (props.field.type === 'array' || props.field.fields?.length) return true
  const defaults = newTemplateField(props.field.type)
  return (['encoding', 'length', 'maxLength', 'enumLabels', 'bitFlags', 'expect'] as const)
    .some(key => props.field[key] !== undefined && JSON.stringify(props.field[key]) !== JSON.stringify(defaults[key]))
}
function requestRemove(): void {
  const configured = hasTypeConfiguration() || props.field.placement || props.field.align || props.field.condition || props.field.comment || !/^field\d+$/.test(props.field.name ?? '')
  if (configured) pending.value = { kind: 'remove', baseline: JSON.stringify(props.field) }
  else emit('remove')
}
function confirmChange(): void {
  const change = pending.value
  pending.value = null
  if (!change || change.baseline !== JSON.stringify(props.field)) return
  if (change.kind === 'remove') emit('remove')
  else if (change.next) emit('update:field', change.next)
}

function patch(value: Partial<TemplateField>): void { emit('update:field', { ...props.field, ...value }) }
function chooseType(type: FieldType): void {
  if (type === props.field.type) return
  const next = newTemplateField(type, props.field.name)
  if (props.field.placement) next.placement = props.field.placement
  if (props.field.align !== undefined) next.align = props.field.align
  if (props.field.comment !== undefined) next.comment = props.field.comment
  if (props.field.condition) next.condition = props.field.condition
  if (hasEndian.value && (type === 'struct' || type === 'array' || /^([ui])(8|16|32|64)$/.test(type) || type === 'f32' || type === 'f64')) next.endianness = props.field.endianness
  if (hasTypeConfiguration()) pending.value = { kind: 'type', next, baseline: JSON.stringify(props.field) }
  else emit('update:field', next)
}
function placementMode(mode: 'absolute' | 'relative' | 'sequential'): void {
  patch({ placement: mode === 'sequential' ? undefined : { mode, offset: props.field.placement?.offset ?? '0' } })
}
function lengthMode(mode: 'fixed' | 'nullTerminated'): void {
  const bound = typeof props.field.length === 'object' ? props.field.length.max : props.field.length ?? props.field.maxLength ?? 1
  emit('update:field', { ...props.field, length: mode === 'fixed' ? bound : undefined, maxLength: mode === 'nullTerminated' ? bound : undefined })
}
function countKind(mode: 'fixed' | 'ref'): void { patch({ count: mode === 'fixed' ? { fixed: 1 } : { ref: '' } }) }
function lengthSource(mode: 'fixed' | 'ref'): void { patch({ length: mode === 'ref' ? { ref: '', max: 256 } : 1 }) }
function offsetSource(mode: 'fixed' | 'ref'): void { patch({ placement: { ...props.field.placement!, offset: mode === 'ref' ? { ref: '' } : '0' } }) }
function toggleCondition(checked: boolean): void {
  patch({ condition: checked ? { ref: '', op: 'eq', value: { type: 'unsigned', value: '0' } } : undefined })
}
function conditionKind(type: ConditionValue['type']): void {
  const value: ConditionValue = type === 'bool' ? { type, value: false } : { type, value: '0' }
  patch({ condition: { ref: props.field.condition?.ref ?? '', op: props.field.condition?.op ?? 'eq', value } })
}
function updateChild(index: number, child: TemplateField): void {
  patch({ fields: props.field.fields?.map((item, at) => at === index ? child : item) })
}
function removeChild(index: number): void { patch({ fields: props.field.fields?.filter((_, at) => at !== index) }) }
function duplicateChild(index: number): void { patch({ fields: duplicateTemplateField(props.field.fields ?? [], index) }) }
function moveChild(index: number, delta: -1 | 1): void {
  const children = [...(props.field.fields ?? [])]
  const other = index + delta
  if (other < 0 || other >= children.length) return
  ;[children[index], children[other]] = [children[other]!, children[index]!]
  patch({ fields: children })
}
function addChild(): void {
  const children = props.field.fields ?? []
  let ordinal = children.length + 1
  while (children.some(child => child.name === `field${ordinal}`)) ordinal += 1
  patch({ fields: [...children, newTemplateField('u8', `field${ordinal}`)] })
}
function updateEnumKey(oldKey: string, nextKey: string): void {
  const labels = { ...props.field.enumLabels }
  const label = labels[oldKey] ?? ''
  delete labels[oldKey]
  labels[nextKey] = label
  patch({ enumLabels: labels })
}
function updateEnumLabel(key: string, label: string): void { patch({ enumLabels: { ...props.field.enumLabels, [key]: label } }) }
function addEnum(): void {
  const labels = { ...props.field.enumLabels }
  let index = 0
  while (String(index) in labels) index += 1
  labels[String(index)] = `Label ${index}`
  patch({ enumLabels: labels })
}
function removeEnum(key: string): void {
  const labels = { ...props.field.enumLabels }
  delete labels[key]
  patch({ enumLabels: Object.keys(labels).length ? labels : undefined })
}
function updateFlag(index: number, value: { bit: number; name: string }): void { patch({ bitFlags: props.field.bitFlags?.map((flag, at) => at === index ? value : flag) }) }
function addFlag(): void {
  const flags = props.field.bitFlags ?? []
  let bit = 0
  while (flags.some(flag => flag.bit === bit)) bit += 1
  patch({ bitFlags: [...flags, { bit, name: `flag${bit}` }] })
}
function removeFlag(index: number): void {
  const flags = props.field.bitFlags?.filter((_, at) => at !== index) ?? []
  patch({ bitFlags: flags.length ? flags : undefined })
}
</script>

<template>
  <article class="field-card" :style="{ '--nest-depth': depth }" :data-field-path="path">
    <div class="field-heading">
      <input v-if="!isElement" data-field="name" aria-label="Field name" :value="field.name ?? ''" @input="patch({ name: ($event.target as HTMLInputElement).value })">
      <strong v-else>Array element</strong>
      <select data-field="type" aria-label="Field type" :value="field.type" @change="chooseType(($event.target as HTMLSelectElement).value as FieldType)">
        <option v-for="choice in choices" :key="choice" :value="choice">{{ choice }}</option>
      </select>
      <div v-if="!isElement" class="card-tools">
        <button type="button" data-action="move-field-up" title="Move up" :disabled="index === 0" @click="emit('move', -1)">↑</button>
        <button type="button" data-action="move-field-down" title="Move down" :disabled="index === siblingCount - 1" @click="emit('move', 1)">↓</button>
        <button type="button" data-action="duplicate-field" title="Duplicate field" @click="emit('duplicate')">⧉</button>
        <button type="button" data-action="remove-field" title="Remove field" @click="requestRemove">×</button>
      </div>
    </div>
    <div class="field-grid">
      <label v-if="!isElement">Placement
        <select data-field="placement-mode" :value="field.placement?.mode ?? 'sequential'" @change="placementMode(($event.target as HTMLSelectElement).value as 'absolute' | 'relative' | 'sequential')">
          <option value="sequential">Sequential</option><option value="relative">Relative to struct</option><option value="absolute">Absolute</option>
        </select>
      </label>
      <template v-if="field.placement && field.placement.mode !== 'sequential'">
        <label>Offset source<select data-field="offset-source" :value="offsetReference ? 'ref' : 'fixed'" @change="offsetSource(($event.target as HTMLSelectElement).value as 'fixed' | 'ref')"><option value="fixed">Decimal offset</option><option value="ref">Earlier unsigned field</option></select></label>
        <label v-if="!offsetReference">Offset<input data-field="placement-offset" :value="field.placement.offset ?? ''" @input="patch({ placement: { ...field.placement!, offset: ($event.target as HTMLInputElement).value } })"></label>
        <template v-else>
          <label>Offset field<input data-field="offset-ref" :list="`${listId}-unsigned`" :value="offsetReference.ref" placeholder="header.offset" @input="patch({ placement: { ...field.placement!, offset: { ...offsetReference!, ref: ($event.target as HTMLInputElement).value } } })"></label>
          <label>Add bytes<input data-field="offset-add" :value="offsetReference.add ?? ''" placeholder="0" @input="patch({ placement: { ...field.placement!, offset: { ...offsetReference!, add: ($event.target as HTMLInputElement).value || undefined } } })"></label>
        </template>
      </template>
      <label v-if="hasEndian">Endianness
        <select data-field="endianness" :value="field.endianness ?? ''" @change="patch({ endianness: (($event.target as HTMLSelectElement).value || undefined) as Endian | undefined })">
          <option value="">Inherit</option><option value="little">Little</option><option value="big">Big</option>
        </select>
      </label>
      <label v-if="field.type === 'bytes' && !lengthReference">Length bytes<input data-field="length" type="number" min="1" step="1" :value="field.length ?? 1" @input="patch({ length: Number(($event.target as HTMLInputElement).value) })"></label>
      <template v-if="field.type === 'string'">
        <label>Encoding<select data-field="encoding" :value="field.encoding ?? 'utf8'" @change="patch({ encoding: ($event.target as HTMLSelectElement).value as TemplateField['encoding'] })"><option value="ascii">ASCII</option><option value="utf8">UTF-8</option><option value="utf16le">UTF-16LE</option><option value="utf16be">UTF-16BE</option></select></label>
        <label>Text bound<select data-field="text-mode" :value="textMode" @change="lengthMode(($event.target as HTMLSelectElement).value as 'fixed' | 'nullTerminated')"><option value="fixed">Fixed length</option><option value="nullTerminated">Null-terminated</option></select></label>
        <label v-if="!lengthReference">{{ textMode === 'fixed' ? 'Length bytes' : 'Maximum bytes' }}<input data-field="text-length" type="number" min="1" step="1" :value="field.length ?? field.maxLength ?? 1" @input="textMode === 'fixed' ? patch({ length: Number(($event.target as HTMLInputElement).value) }) : patch({ maxLength: Number(($event.target as HTMLInputElement).value) })"></label>
      </template>
      <template v-if="field.type === 'bytes' || (field.type === 'string' && textMode === 'fixed')">
        <label>Length source<select data-field="length-source" :value="lengthReference ? 'ref' : 'fixed'" @change="lengthSource(($event.target as HTMLSelectElement).value as 'fixed' | 'ref')"><option value="fixed">Fixed length</option><option value="ref">Earlier unsigned field</option></select></label>
        <template v-if="lengthReference">
          <label>Length field<input data-field="length-ref" :list="`${listId}-unsigned`" :value="lengthReference.ref" placeholder="header.length" @input="patch({ length: { ...lengthReference!, ref: ($event.target as HTMLInputElement).value } })"></label>
          <label>Maximum bytes<input data-field="length-max" type="number" min="1" max="1048576" :value="lengthReference.max" @input="patch({ length: { ...lengthReference!, max: Number(($event.target as HTMLInputElement).value) } })"></label>
        </template>
      </template>
      <template v-if="field.type === 'array'">
        <label>Count source<select data-field="count-mode" :value="countMode" @change="countKind(($event.target as HTMLSelectElement).value as 'fixed' | 'ref')"><option value="fixed">Fixed count</option><option value="ref">Earlier unsigned field</option></select></label>
        <label v-if="countMode === 'fixed'">Count<input data-field="count-fixed" type="number" min="0" step="1" :value="field.count?.fixed ?? 0" @input="patch({ count: { fixed: Number(($event.target as HTMLInputElement).value) } })"></label>
        <label v-else>Field path<input data-field="count-ref" :list="`${listId}-unsigned`" placeholder="header.count" :value="field.count?.ref ?? ''" @input="patch({ count: { ref: ($event.target as HTMLInputElement).value } })"></label>
      </template>
    </div>
    <details data-testid="field-advanced" class="advanced" :open="advanced" @toggle="advanced = ($event.target as HTMLDetailsElement).open">
      <summary>Advanced<span v-if="advancedSummary"> · {{ advancedSummary }}</span></summary>
      <div class="field-grid">
        <label>Align bytes<input data-field="align" type="number" min="1" step="1" placeholder="None" :value="field.align ?? ''" @input="patch({ align: ($event.target as HTMLInputElement).value ? Number(($event.target as HTMLInputElement).value) : undefined })"></label>
      </div>
    <div v-if="!isElement" class="condition-row">
      <label class="condition-check"><input type="checkbox" data-action="condition-toggle" :checked="Boolean(field.condition)" @change="toggleCondition(($event.target as HTMLInputElement).checked)"> Condition</label>
      <template v-if="field.condition">
        <input data-field="condition-ref" :list="`${listId}-condition`" aria-label="Condition field path" placeholder="Earlier field path" :value="field.condition.ref" @input="patch({ condition: { ...field.condition!, ref: ($event.target as HTMLInputElement).value } })">
        <select data-field="condition-op" aria-label="Comparison" :value="field.condition.op" @change="patch({ condition: { ...field.condition!, op: ($event.target as HTMLSelectElement).value as typeof field.condition.op } })"><option v-for="op in ['eq', 'ne', 'lt', 'lte', 'gt', 'gte']" :key="op" :value="op">{{ op }}</option></select>
        <select data-field="condition-type" aria-label="Condition type" :value="field.condition.value.type" @change="conditionKind(($event.target as HTMLSelectElement).value as ConditionValue['type'])"><option value="unsigned">Unsigned</option><option value="signed">Signed</option><option value="bool">Bool</option><option value="string">Text</option></select>
        <select v-if="field.condition.value.type === 'bool'" data-field="condition-value" :value="String(field.condition.value.value)" @change="patch({ condition: { ...field.condition!, value: { type: 'bool', value: ($event.target as HTMLSelectElement).value === 'true' } } })"><option value="false">false</option><option value="true">true</option></select>
        <input v-else data-field="condition-value" aria-label="Comparison value" :value="field.condition.value.value" @input="patch({ condition: { ...field.condition!, value: { ...field.condition!.value, value: ($event.target as HTMLInputElement).value } as ConditionValue } })">
      </template>
    </div>
    <ExpectationEditor v-if="field.type !== 'struct' && field.type !== 'array'" :type="field.type" :model-value="field.expect" @update:model-value="patch({ expect: $event })" />
    <div v-if="integer" class="metadata-row">
      <details><summary>Enum labels</summary>
        <div v-for="(label, key) in field.enumLabels" :key="key" class="metadata-entry"><input aria-label="Enum value" :value="key" @change="updateEnumKey(String(key), ($event.target as HTMLInputElement).value)"><input aria-label="Enum label" :value="label" @input="updateEnumLabel(String(key), ($event.target as HTMLInputElement).value)"><button type="button" title="Remove enum label" @click="removeEnum(String(key))">×</button></div>
        <button type="button" data-action="add-enum" @click="addEnum">+ Label</button>
      </details>
      <details v-if="unsigned"><summary>Bit flags</summary>
        <div v-for="(flag, flagIndex) in field.bitFlags" :key="flagIndex" class="metadata-entry"><input aria-label="Bit number" type="number" min="0" :value="flag.bit" @input="updateFlag(flagIndex, { ...flag, bit: Number(($event.target as HTMLInputElement).value) })"><input aria-label="Flag name" :value="flag.name" @input="updateFlag(flagIndex, { ...flag, name: ($event.target as HTMLInputElement).value })"><button type="button" title="Remove flag" @click="removeFlag(flagIndex)">×</button></div>
        <button type="button" data-action="add-flag" @click="addFlag">+ Flag</button>
      </details>
    </div>
    <input class="comment" data-field="comment" aria-label="Comment" placeholder="Comment" :value="field.comment ?? ''" @input="patch({ comment: ($event.target as HTMLInputElement).value })">
    </details>
    <datalist :id="`${listId}-unsigned`"><option v-for="item in unsignedRefs" :key="item.path" :value="item.ref">{{ item.path }} · {{ item.type }}</option></datalist>
    <datalist :id="`${listId}-condition`"><option v-for="item in conditionRefs" :key="item.path" :value="item.ref">{{ item.path }} · {{ item.type }}</option></datalist>
    <div v-if="field.type === 'struct'" class="nested-fields">
      <div class="nested-heading"><span>Structure fields</span><button type="button" data-action="add-child-field" @click="addChild">+ Field</button></div>
      <FieldCard v-for="(child, childIndex) in field.fields ?? []" :key="childIndex" :field="child" :definition="definition" :path="`${path}.${child.name ?? ''}`" :index="childIndex" :sibling-count="field.fields?.length ?? 0" :depth="depth + 1" :issues="issues" :preview="preview"
        @update:field="updateChild(childIndex, $event)" @remove="removeChild(childIndex)" @duplicate="duplicateChild(childIndex)" @move="moveChild(childIndex, $event)" @navigate="emit('navigate', $event)" />
    </div>
    <div v-if="field.type === 'array' && field.element" class="nested-fields">
      <div class="nested-heading"><span>Array element definition</span></div>
      <FieldCard :field="field.element" :definition="definition" :path="`${path}[]`" :depth="depth + 1" is-element :issues="issues" :preview="preview" @update:field="patch({ element: $event })" @navigate="emit('navigate', $event)" />
    </div>
    <p v-for="(issue, issueIndex) in fieldIssues" :key="issueIndex" data-testid="field-error" class="field-error" role="alert">{{ issue.message }}</p>
    <button v-if="matchedPreview" type="button" class="preview-link" data-action="navigate-field" @click="emit('navigate', path)">↗ {{ matchedPreview.value }} · {{ matchedPreview.offset }}</button>
    <AppDialog :open="pending !== null" :title="pending?.kind === 'remove' ? 'Remove field?' : 'Change field type?'"
      :message="`This will remove ${pending?.kind === 'remove' ? 'the field' : 'type-specific configuration'} for '${field.name ?? 'array element'}', including nested fields or rules. This cannot be undone.`" @close="pending = null">
      <div class="confirm-actions">
        <button type="button" data-action="confirm-field-change" @click="confirmChange">{{ pending?.kind === 'remove' ? 'Remove Field' : 'Change Type' }}</button>
        <button type="button" data-action="cancel-field-change" autofocus @click="pending = null">Cancel</button>
      </div>
    </AppDialog>
  </article>
</template>

<style scoped>
.field-card { display: grid; gap: 8px; min-width: 0; padding: 10px; margin-bottom: 8px; background: var(--card); border: 1px solid var(--border); border-radius: 5px; }
.field-heading { display: flex; align-items: center; gap: 7px; min-width: 0; }
.field-heading > input { flex: 1; }
.field-heading > select { width: 118px; }
.field-heading strong { flex: 1; color: var(--muted); font-size: var(--font-support); }
.card-tools, .metadata-row { display: flex; gap: 4px; }
.field-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 7px; }
.field-grid label { display: grid; gap: 3px; color: var(--muted); font-size: var(--font-support); }
input, select { box-sizing: border-box; min-width: 0; width: 100%; height: 28px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 4px; font: inherit; font-size: var(--font-body); }
input:focus, select:focus { outline: 1px solid var(--selection); }
button { min-height: 27px; padding: 0 8px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-support); white-space: nowrap; }
button:hover:not(:disabled) { background: var(--hover); }
button:disabled { opacity: .4; }
.card-tools button { width: 25px; padding: 0; font-size: var(--font-body); }
.condition-row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.condition-row > input { flex: 1; min-width: 120px; }
.condition-row > select { width: auto; }
.condition-check { display: flex; align-items: center; gap: 5px; color: var(--muted); font-size: var(--font-support); }
.condition-check input { width: 15px; height: 15px; }
.metadata-row details { flex: 1; min-width: 0; color: var(--muted); font-size: var(--font-support); }
.metadata-row summary { cursor: pointer; }
.metadata-entry { display: flex; gap: 4px; margin: 4px 0; }
.metadata-entry input:first-child { width: 85px; }
.nested-fields { padding: 8px 0 0 9px; border-left: 2px solid var(--border-strong); }
.nested-heading { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; color: var(--muted); font-size: var(--font-support); }
.comment { width: 100%; }
.field-error { margin: 0; color: var(--modified); font-size: var(--font-support); }
.preview-link { justify-self: start; color: var(--address); background: transparent; }
.advanced { display: grid; color: var(--muted); font-size: var(--font-support); }
.advanced > summary { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
.advanced > summary span { color: var(--address); }
.advanced > div, .advanced > input { margin-top: 8px; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 14px; }
@media (max-width: 620px) { .field-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
