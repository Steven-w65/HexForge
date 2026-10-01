import type { ConditionValue, FieldType, TemplateDefinition, TemplateField } from '../types'

export interface ReferenceCandidate { ref: string; path: string; type: FieldType }
function parent(path: string): string | null {
  if (path.endsWith(']')) return path.slice(0, path.lastIndexOf('['))
  const dot = path.lastIndexOf('.')
  return dot < 0 ? null : path.slice(0, dot)
}

/** Suggestions are advisory; Rust still validates instantiated/conditional references. */
export function referenceCandidates(template: TemplateDefinition, target: string, literal: ConditionValue['type']): ReferenceCandidate[] {
  const seen = new Map<string, FieldType>()
  let stopped = false
  function visit(field: TemplateField, path: string): void {
    if (stopped) return
    if (path === target) { stopped = true; return }
    if (field.type === 'struct') field.fields?.forEach(child => visit(child, `${path}.${child.name ?? ''}`))
    else if (field.type === 'array' && field.element) visit(field.element, `${path}[]`)
    else if (/^[ui](8|16|32|64)$/.test(field.type) || field.type === 'bool' || field.type === 'string') seen.set(path, field.type)
  }
  template.fields.forEach(field => visit(field, field.name ?? ''))
  if (!stopped) return []
  const scopes: string[] = []
  for (let scope = parent(target); scope !== null; scope = parent(scope)) scopes.push(scope)
  scopes.push('')
  function resolve(ref: string): string | undefined {
    for (const scope of scopes) {
      const path = scope ? `${scope}.${ref}` : ref
      if (seen.has(path)) return path
    }
    return undefined
  }
  const candidates: ReferenceCandidate[] = []
  for (const scope of scopes) for (const [path, type] of seen) {
    if (literal === 'unsigned' ? !/^u(8|16|32|64)$/.test(type) : literal === 'signed' ? !/^i(8|16|32|64)$/.test(type) : type !== (literal === 'string' ? 'string' : 'bool')) continue
    if (scope && !path.startsWith(`${scope}.`)) continue
    // Another repeated structure has no single representative runtime value.
    if (path.includes('[]') && !scopes.some(value => value.includes('[]') && path.startsWith(`${value}.`))) continue
    const ref = scope ? path.slice(scope.length + 1) : path
    if (ref.includes('[]') || resolve(ref) !== path || candidates.some(item => item.path === path)) continue
    candidates.push({ ref, path, type })
  }
  return candidates
}
