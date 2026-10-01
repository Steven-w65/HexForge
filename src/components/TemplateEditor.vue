<script setup lang="ts">
import { computed, watch } from 'vue'
import type { ParsedResult, TemplateDefinition, TemplateField } from '../types'
import { duplicateTemplateField, flattenResultLeaves, newTemplateField, validateTemplate } from '../template/model'
import FieldCard from './FieldCard.vue'
import { commandUnavailableReason, type MenuCommand, type MenuState } from '../menu/commands'

const props = withDefaults(defineProps<{
  modelValue: TemplateDefinition
  results?: ParsedResult[]
  canApply?: boolean
  canUnload?: boolean
  canSave?: boolean
  canSaveAs?: boolean
  menuState?: MenuState
}>(), { results: () => [], canApply: false, canUnload: false, canSave: false, canSaveAs: false })
const emit = defineEmits<{
  'update:modelValue': [value: TemplateDefinition]
  validity: [valid: boolean]
  save: []; 'save-as': []; load: []; unload: []; apply: []
  navigate: [range: { start: bigint; end: bigint }]
}>()

const issues = computed(() => validateTemplate(props.modelValue))
const preview = computed(() => flattenResultLeaves(props.results))
function actionReason(command: MenuCommand): string | undefined {
  if (props.menuState) return commandUnavailableReason(command, { ...props.menuState, templateValid: issues.value.length === 0, templateIssueCount: issues.value.length }) ?? undefined
  if (['save-template', 'save-template-as', 'apply-template'].includes(command) && issues.value.length) return `Correct ${issues.value.length} template errors first.`
  if (command === 'save-template' && !props.canSave) return 'Use Save As first to choose a template file.'
  if (command === 'save-template-as' && !props.canSaveAs) return 'Load or create a template first.'
  if (command === 'apply-template' && !props.canApply) return 'Open a binary file and add valid fields to apply.'
  if (command === 'unload-template' && !props.canUnload) return 'No template is loaded.'
  return undefined
}
watch(issues, value => emit('validity', value.length === 0), { immediate: true })

function update(patch: Partial<TemplateDefinition>): void { emit('update:modelValue', { ...props.modelValue, ...patch }) }
function updateField(index: number, value: TemplateField): void { update({ fields: props.modelValue.fields.map((field, at) => at === index ? value : field) }) }
function addField(): void {
  let ordinal = props.modelValue.fields.length + 1
  while (props.modelValue.fields.some(field => field.name === `field${ordinal}`)) ordinal += 1
  update({ fields: [...props.modelValue.fields, newTemplateField('u8', `field${ordinal}`)] })
}
defineExpose({ addField })
function removeField(index: number): void { update({ fields: props.modelValue.fields.filter((_, at) => at !== index) }) }
function duplicateField(index: number): void { update({ fields: duplicateTemplateField(props.modelValue.fields, index) }) }
function moveField(index: number, delta: -1 | 1): void {
  const fields = [...props.modelValue.fields]
  const other = index + delta
  if (other < 0 || other >= fields.length) return
  ;[fields[index], fields[other]] = [fields[other]!, fields[index]!]
  update({ fields })
}
function navigate(path: string): void {
  const leaf = preview.value.find(value => value.path === path)
  if (!leaf) return
  const start = BigInt(leaf.offset)
  emit('navigate', { start, end: start + BigInt(leaf.length) - 1n })
}
</script>

<template>
  <section class="template-editor">
    <div class="template-meta">
      <label>Name<input data-field="template-name" :value="modelValue.name" @input="update({ name: ($event.target as HTMLInputElement).value })"></label>
      <label>Default endian<select :value="modelValue.defaultEndianness" @change="update({ defaultEndianness: ($event.target as HTMLSelectElement).value as 'little' | 'big' })"><option value="little">Little</option><option value="big">Big</option></select></label>
    </div>
    <p class="editor-note">Fields parse in order. References use earlier result paths.</p>
    <div class="field-list">
      <FieldCard v-for="(field, index) in modelValue.fields" :key="index" :field="field" :definition="modelValue" :path="field.name ?? ''" :index="index" :sibling-count="modelValue.fields.length" :issues="issues" :preview="preview"
        @update:field="updateField(index, $event)" @remove="removeField(index)" @duplicate="duplicateField(index)" @move="moveField(index, $event)" @navigate="navigate" />
      <p v-if="modelValue.fields.length === 0" class="empty">No fields defined. Add a field to start a template.</p>
      <p v-for="(issue, index) in issues.filter(value => value.path === 'template')" :key="index" role="alert" class="error">{{ issue.message }}</p>
    </div>
    <div class="template-actions">
      <div>
        <button type="button" data-action="add-field" @click="addField">+ Field</button>
        <button type="button" data-action="load-template" :disabled="Boolean(actionReason('load-template'))" :title="actionReason('load-template')" @click="emit('load')">Load…</button>
        <button type="button" data-action="unload-template" :disabled="Boolean(actionReason('unload-template'))" :title="actionReason('unload-template')" @click="emit('unload')">Unload</button>
        <button type="button" data-action="save-template" :disabled="Boolean(actionReason('save-template'))" :title="actionReason('save-template')" @click="emit('save')">Save</button>
        <button type="button" data-action="save-template-as" :disabled="Boolean(actionReason('save-template-as'))" :title="actionReason('save-template-as')" @click="emit('save-as')">Save As…</button>
      </div>
      <button type="button" class="primary" data-action="apply-template" :disabled="Boolean(actionReason('apply-template'))" :title="actionReason('apply-template')" @click="emit('apply')">Apply Template</button>
    </div>
  </section>
</template>

<style scoped>
.template-editor { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 9px; }
.template-meta { display: grid; grid-template-columns: minmax(0, 1fr) 150px; gap: 9px; }
label { display: grid; gap: 3px; color: var(--muted); font-size: var(--font-support); }
input, select { box-sizing: border-box; min-width: 0; width: 100%; height: 28px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 4px; font: inherit; font-size: var(--font-body); }
input:focus, select:focus { outline: 1px solid var(--selection); }
.editor-note { margin: 0; color: var(--address); font-size: var(--font-support); }
.field-list { flex: 1; min-height: 0; padding-right: 3px; overflow: auto; }
.empty { color: var(--muted); font-size: var(--font-body); }
.error { color: var(--modified); font-size: var(--font-support); }
.template-actions { display: flex; justify-content: space-between; gap: 8px; padding-top: 10px; border-top: 1px solid var(--border); }
.template-actions > div { display: flex; flex-wrap: wrap; gap: 6px; }
button { height: 27px; padding: 0 9px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); }
button:hover:not(:disabled) { background: var(--hover); }
button:disabled { opacity: .4; }
.primary { color: var(--selection-text); background: var(--selection); }
.primary:hover:not(:disabled) { filter: brightness(1.12); }
</style>
