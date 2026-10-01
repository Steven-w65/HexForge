import { toRaw } from 'vue'
import type { TemplateField } from '../types'

// Sidecar identity: never add editor state to JSON or backend snapshots. Vue
// proxies and immutable replacements refer to the same logical field card.
const ids = new WeakMap<TemplateField, string>()
let nextId = 0
export function fieldId(field: TemplateField): string {
  const raw = toRaw(field)
  let id = ids.get(raw)
  if (!id) { id = `field-${++nextId}`; ids.set(raw, id) }
  return id
}
export function inheritFieldIdentity(source: TemplateField, replacement: TemplateField): TemplateField {
  ids.set(toRaw(replacement), fieldId(source))
  return replacement
}
export function copyEditorField(field: TemplateField): TemplateField {
  const copy = JSON.parse(JSON.stringify(field)) as TemplateField
  const transfer = (from: TemplateField, to: TemplateField): void => {
    inheritFieldIdentity(from, to)
    from.fields?.forEach((child, index) => { if (to.fields?.[index]) transfer(child, to.fields[index]!) })
    if (from.element && to.element) transfer(from.element, to.element)
  }
  transfer(field, copy)
  return copy
}
