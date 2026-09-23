import { beforeEach, describe, expect, it } from 'vitest'
import { useTheme } from './useTheme'

describe('useTheme', () => {
  beforeEach(() => { delete document.documentElement.dataset.theme })

  it('defaults to dark and toggles the root data attribute', () => {
    const theme = useTheme(document.documentElement)
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    theme.toggle()
    expect(theme.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
  })

  it('applies a deterministic menu-selected theme', () => {
    const theme = useTheme(document.documentElement)
    theme.apply('light')
    expect(theme.value.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
    theme.apply('dark')
    expect(theme.value.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })
})
