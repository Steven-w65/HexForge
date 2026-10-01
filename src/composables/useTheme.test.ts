import { effectScope, type EffectScope } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useTheme } from './useTheme'
import { installSystemTheme } from '../../tests/helpers/systemTheme'

const scopes: EffectScope[] = []
function createTheme() {
  const scope = effectScope()
  scopes.push(scope)
  return scope.run(() => useTheme(document.documentElement))!
}

describe('useTheme', () => {
  beforeEach(() => { delete document.documentElement.dataset.theme })
  afterEach(() => {
    scopes.splice(0).forEach(scope => scope.stop())
    vi.unstubAllGlobals()
  })

  it.each([
    { dark: false, expected: 'light', stale: 'dark' },
    { dark: true, expected: 'dark', stale: 'light' },
  ])('starts with the system theme ($expected), not a stale root theme', ({ dark, expected, stale }) => {
    installSystemTheme(dark)
    document.documentElement.dataset.theme = stale
    const theme = createTheme()
    expect(theme.value.value).toBe(expected)
    expect(document.documentElement.dataset.theme).toBe(expected)
  })

  it('updates the root and reactive theme when the system changes in either direction', () => {
    const system = installSystemTheme(true)
    const theme = createTheme()
    system.setDark(false)
    expect(theme.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
    system.setDark(true)
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('keeps a manual toggle until the next system change, then follows the system again', () => {
    const system = installSystemTheme(true)
    const theme = createTheme()
    theme.toggle()
    expect(theme.value.value).toBe('light')
    system.setDark(true) // No actual OS change: retain the manual choice.
    expect(theme.value.value).toBe('light')
    system.setDark(false)
    system.setDark(true)
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('starts a new app scope in the system theme rather than retaining a manual override', () => {
    installSystemTheme(false)
    const first = createTheme()
    first.toggle()
    expect(first.value.value).toBe('dark')
    scopes[0]!.stop()
    const reopened = createTheme()
    expect(reopened.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
  })

  it('stops responding to system changes when its app scope is disposed', () => {
    const system = installSystemTheme(true)
    const theme = createTheme()
    scopes[0]!.stop()
    system.setDark(false)
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('falls back to dark and keeps manual toggling if matchMedia is unavailable', () => {
    vi.stubGlobal('matchMedia', undefined)
    const theme = createTheme()
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    theme.toggle()
    expect(theme.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
  })

  it('applies a deterministic menu-selected theme', () => {
    const theme = createTheme()
    theme.apply('light')
    expect(theme.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
    theme.apply('dark')
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })
})
