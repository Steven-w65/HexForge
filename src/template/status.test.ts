import { describe, expect, it } from 'vitest'
import { templateStatus } from './status'

describe('independent template status', () => {
  it('keeps persistence separate from result freshness', () => {
    const base = { source: 'file' as const, dirty: false, applied: false, needsRefresh: false, hasFile: true, hasDiagnostics: false }
    expect(templateStatus(base)).toBe('Saved · Pending apply')
    expect(templateStatus({ ...base, dirty: true, applied: true })).toBe('Modified · Applied')
    expect(templateStatus({ ...base, dirty: true, needsRefresh: true })).toBe('Modified · Needs apply')
    expect(templateStatus({ ...base, needsRefresh: true })).toBe('Saved · Needs apply')
    expect(templateStatus({ ...base, applied: true, hasDiagnostics: true })).toBe('Saved · Parse errors')
    expect(templateStatus({ ...base, hasFile: false })).toBe('Saved · Loaded')
    expect(templateStatus({ ...base, source: 'draft', applied: true })).toBe('Unsaved draft · Applied')
    expect(templateStatus({ ...base, source: 'none' })).toBe('No template')
  })
})
