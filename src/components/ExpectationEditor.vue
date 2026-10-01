<script setup lang="ts">
import { computed } from 'vue'
import type { Expectation, ExpectedValue, FieldType } from '../types'
const props = defineProps<{ type: FieldType; modelValue?: Expectation }>()
const emit = defineEmits<{ 'update:modelValue': [value: Expectation | undefined] }>()
const numeric = computed(() => /^([ui])(8|16|32|64)$/.test(props.type) || ['f32', 'f64'].includes(props.type))
function literal(input?: string): ExpectedValue {
  const value = input ?? (props.type === 'bytes' ? '00' : props.type === 'string' ? '' : '0')
  if (props.type === 'bool') return { type: 'bool', value: value === 'true' }
  return { type: props.type.startsWith('u') ? 'unsigned' : props.type.startsWith('i') ? 'signed' : props.type.startsWith('f') ? 'float' : props.type === 'bytes' ? 'bytes' : 'string', value }
}
function kind(kind: Expectation['kind']): void {
  emit('update:modelValue', kind === 'equals' ? { kind, value: literal() } : kind === 'oneOf' ? { kind, values: [literal()] } : { kind, min: literal(), max: literal() })
}
function update(value: string, index = 0, edge?: 'min' | 'max'): void {
  const check = props.modelValue
  if (!check) return
  if (check.kind === 'equals') emit('update:modelValue', { ...check, value: literal(value) })
  else if (check.kind === 'range' && edge) emit('update:modelValue', { ...check, [edge]: literal(value) })
  else if (check.kind === 'oneOf') emit('update:modelValue', { ...check, values: check.values.map((item, at) => at === index ? literal(value) : item) })
}
function add(): void { if (props.modelValue?.kind === 'oneOf') emit('update:modelValue', { ...props.modelValue, values: [...props.modelValue.values, literal()] }) }
function remove(index: number): void { if (props.modelValue?.kind === 'oneOf') emit('update:modelValue', { ...props.modelValue, values: props.modelValue.values.filter((_, at) => at !== index) }) }
</script>

<template>
  <div class="expectation-editor">
    <label class="toggle"><input type="checkbox" data-action="expect-toggle" :checked="Boolean(modelValue)" @change="($event.target as HTMLInputElement).checked ? kind('equals') : emit('update:modelValue', undefined)"> Expected value check</label>
    <div v-if="modelValue" class="controls">
      <label>Check<select data-field="expect-kind" :value="modelValue.kind" @change="kind(($event.target as HTMLSelectElement).value as Expectation['kind'])"><option value="equals">Equals</option><option value="oneOf">One of</option><option v-if="numeric" value="range">Inclusive range</option></select></label>
      <label v-if="modelValue.kind === 'equals'">Expected {{ type === 'bytes' ? 'hex bytes' : 'value' }}
        <select v-if="type === 'bool'" data-field="expect-value" :value="String(modelValue.value.value)" @change="update(($event.target as HTMLSelectElement).value)"><option value="false">false</option><option value="true">true</option></select>
        <input v-else data-field="expect-value" :value="modelValue.value.value" @input="update(($event.target as HTMLInputElement).value)">
      </label>
      <template v-else-if="modelValue.kind === 'range'">
        <label>Minimum<input data-field="expect-min" :value="modelValue.min.value" @input="update(($event.target as HTMLInputElement).value, 0, 'min')"></label>
        <label>Maximum<input data-field="expect-max" :value="modelValue.max.value" @input="update(($event.target as HTMLInputElement).value, 0, 'max')"></label>
      </template>
      <div v-else class="allowed-values">
        <div v-for="(value, index) in modelValue.values" :key="index" class="value-row">
          <select v-if="type === 'bool'" aria-label="Allowed value" :value="String(value.value)" @change="update(($event.target as HTMLSelectElement).value, index)"><option value="false">false</option><option value="true">true</option></select>
          <input v-else aria-label="Allowed value" data-field="expect-allowed" :value="value.value" @input="update(($event.target as HTMLInputElement).value, index)">
          <button type="button" aria-label="Remove allowed value" @click="remove(index)">×</button>
        </div>
        <button type="button" data-action="add-expected-value" :disabled="modelValue.values.length >= 256" @click="add">+ Value</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.expectation-editor { display: grid; gap: 6px; color: var(--muted); font-size: var(--font-support); }
.toggle { display: flex; align-items: center; gap: 5px; }
.toggle input { width: 15px; height: 15px; }
.controls { display: flex; flex-wrap: wrap; gap: 7px; }
.controls > label { display: grid; flex: 1; min-width: 100px; gap: 3px; }
input, select { box-sizing: border-box; min-width: 0; width: 100%; height: 28px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 4px; font: inherit; font-size: var(--font-body); }
input:focus, select:focus { outline: 1px solid var(--selection); }
.allowed-values { flex: 2; min-width: 140px; }
.value-row { display: flex; gap: 4px; margin-bottom: 4px; }
button { min-height: 27px; padding: 0 8px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; }
button:hover:not(:disabled) { background: var(--hover); }
button:disabled { opacity: .4; }
</style>
