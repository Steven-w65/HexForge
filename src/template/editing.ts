import type { TemplateDefinition, TemplateField } from '../types'
import { copyEditorField, fieldId } from './editorState'

type ReferenceKey = 'count' | 'length' | 'offset' | 'condition'
interface Entry { field: TemplateField; path: string; siblings: TemplateField[] }
interface Binding { owner: string; key: ReferenceKey; ref: string; target: string; fullPath: string; scope: string }
function parent(path: string): string | null {
  if (path.endsWith(']')) return path.slice(0, path.lastIndexOf('['))
  const dot = path.lastIndexOf('.')
  return dot < 0 ? null : path.slice(0, dot)
}
function entries(template: TemplateDefinition): Entry[] {
  const result: Entry[] = []
  const visit = (field: TemplateField, path: string, siblings: TemplateField[]): void => {
    result.push({ field, path, siblings })
    field.fields?.forEach(child => visit(child, `${path}.${child.name ?? ''}`, field.fields!))
    if (field.element) visit(field.element, `${path}[]`, [])
  }
  template.fields.forEach(field => visit(field, field.name ?? '', template.fields))
  return result
}
function references(field: TemplateField): Array<[ReferenceKey, string]> {
  const result: Array<[ReferenceKey, string]> = []
  if (field.count?.ref !== undefined) result.push(['count', field.count.ref])
  if (typeof field.length === 'object') result.push(['length', field.length.ref])
  if (typeof field.placement?.offset === 'object') result.push(['offset', field.placement.offset.ref])
  if (field.condition) result.push(['condition', field.condition.ref])
  return result
}
function compatible(owner: TemplateField, key: ReferenceKey, target: TemplateField): boolean {
  if (key !== 'condition' || owner.condition?.value.type === 'unsigned') return /^u(8|16|32|64)$/.test(target.type)
  const type = owner.condition?.value.type
  return type === 'signed' ? /^i(8|16|32|64)$/.test(target.type) : target.type === type
}

/** Bind symbolic definitions, not expanded arrays. Concrete indices keep their
 * original spelling and are checked against known fixed counts. Conditional
 * existence and dynamic indices remain the interpreter's responsibility. */
function bindings(template: TemplateDefinition): Binding[] {
  const all = new Map(entries(template).map(entry => [entry.path, entry.field]))
  const seen = new Map<string, TemplateField>()
  const result: Binding[] = []
  function lookup(fullPath: string): TemplateField | undefined {
    const pattern = fullPath.replace(/\[\d+\]/g, '[]')
    for (const match of fullPath.matchAll(/\[(\d+)\]/g)) {
      const array = all.get(fullPath.slice(0, match.index).replace(/\[\d+\]/g, '[]'))
      if (!array || array.type !== 'array') return undefined
      const fixed = array.count?.fixed
      if (fixed !== undefined && (!Number.isSafeInteger(fixed) || fixed < 0 || BigInt(match[1]!) >= BigInt(fixed))) return undefined
    }
    return seen.get(pattern)
  }
  function visit(field: TemplateField, path: string): void {
    const scopes: string[] = []
    for (let scope = parent(path); scope !== null; scope = parent(scope)) scopes.push(scope)
    scopes.push('')
    for (const [key, ref] of references(field)) {
      for (const scope of scopes) {
        const fullPath = scope ? `${scope}.${ref}` : ref
        const target = lookup(fullPath)
        if (!target) continue
        // Runtime chooses the nearest value even when its type is wrong.
        if (compatible(field, key, target)) result.push({ owner: fieldId(field), key, ref, target: fieldId(target), fullPath, scope })
        break
      }
    }
    if (field.type === 'struct') field.fields?.forEach(child => visit(child, `${path}.${child.name ?? ''}`))
    else if (field.type === 'array' && field.element) visit(field.element, `${path}[]`)
    else if (/^[ui](8|16|32|64)$/.test(field.type) || field.type === 'bool' || field.type === 'string') seen.set(path, field)
  }
  template.fields.forEach(field => visit(field, field.name ?? ''))
  return result
}
function setReference(field: TemplateField, key: ReferenceKey, ref: string): void {
  if (key === 'count') field.count = { ...field.count, ref }
  else if (key === 'length' && typeof field.length === 'object') field.length = { ...field.length, ref }
  else if (key === 'offset' && field.placement && typeof field.placement.offset === 'object') field.placement = { ...field.placement, offset: { ...field.placement.offset, ref } }
  else if (key === 'condition' && field.condition) field.condition = { ...field.condition, ref }
}
/** Renaming a container rewrites its named path component, preserving every
 * concrete array index and avoiding similarly prefixed sibling names. */
function renamedPath(path: string, prefix: string, name: string): string {
  const tokens = (value: string): string[] => value.match(/[^.\[\]]+|\[\d*\]/g) ?? []
  const parts = tokens(path), original = tokens(prefix)
  if (!original.length) return path
  if (!original.every((part, index) => part === parts[index] || (part === '[]' && /^\[\d*\]$/.test(parts[index] ?? '')))) return path
  parts[original.length - 1] = name
  return parts.reduce((result, part) => result + (part.startsWith('[') || !result ? '' : '.') + part, '')
}

export function renameField(template: TemplateDefinition, id: string, name: string): TemplateDefinition {
  const entry = entries(template).find(item => fieldId(item.field) === id)
  if (!entry || entry.field.name === undefined) throw new Error('This field cannot be renamed.')
  if (!name.trim() || /[.\[\]]/.test(name)) throw new Error('Use a nonempty field name without ., [ or ].')
  if (entry.siblings.some(field => field !== entry.field && field.name === name)) throw new Error(`Field name '${name}' already exists in this structure.`)
  if (name === entry.field.name) return template
  const before = bindings(template)
  const next = { ...template, fields: template.fields.map(copyEditorField) }
  const nextEntries = new Map(entries(next).map(item => [fieldId(item.field), item]))
  nextEntries.get(id)!.field.name = name
  for (const binding of before) {
    const fullPath = renamedPath(binding.fullPath, entry.path, name)
    const scope = renamedPath(binding.scope, entry.path, name)
    const ref = scope && fullPath.startsWith(`${scope}.`) ? fullPath.slice(scope.length + 1) : fullPath
    setReference(nextEntries.get(binding.owner)!.field, binding.key, ref)
  }
  // A new name must not accidentally shadow a different earlier value. Try the
  // fully-qualified path before refusing an ambiguous rename atomically.
  let resolvedBindings = new Map(bindings(next).map(binding => [`${binding.owner}:${binding.key}`, binding]))
  for (const binding of before) {
    let resolved = resolvedBindings.get(`${binding.owner}:${binding.key}`)
    if (resolved?.target !== binding.target) {
      setReference(nextEntries.get(binding.owner)!.field, binding.key, renamedPath(binding.fullPath, entry.path, name))
      resolvedBindings = new Map(bindings(next).map(item => [`${item.owner}:${item.key}`, item]))
      resolved = resolvedBindings.get(`${binding.owner}:${binding.key}`)
      if (resolved?.target !== binding.target) throw new Error(`Rename would change the reference used by ${nextEntries.get(binding.owner)!.path}.`)
    }
  }
  return next
}

/** Return surviving dependent fields whose previously valid binding was lost
 * or retargeted. Already-invalid drafts and intentional reference edits do not
 * produce spurious destructive-change confirmations. */
export function brokenReferences(before: TemplateDefinition, after: TemplateDefinition): string[] {
  const current = new Map(entries(after).map(entry => [fieldId(entry.field), entry]))
  const afterBindings = new Map(bindings(after).map(binding => [`${binding.owner}:${binding.key}`, binding]))
  const affected = new Set<string>()
  for (const binding of bindings(before)) {
    const owner = current.get(binding.owner)
    if (!owner) continue
    const ref = references(owner.field).find(([key]) => key === binding.key)?.[1]
    const resolved = afterBindings.get(`${binding.owner}:${binding.key}`)
    if (ref === binding.ref && resolved?.target !== binding.target) affected.add(owner.path)
  }
  return [...affected]
}
