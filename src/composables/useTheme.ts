import { getCurrentScope, onScopeDispose, ref } from 'vue'
import type { ColorTheme } from '../types'

export function useTheme(root: HTMLElement = document.documentElement) {
  const value = ref<ColorTheme>('dark')
  const system = typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: dark)') : null

  function apply(theme: ColorTheme): void {
    value.value = theme
    root.dataset.theme = theme
  }

  function toggle(): void {
    apply(value.value === 'dark' ? 'light' : 'dark')
  }

  function onSystemChange(event: MediaQueryListEvent): void {
    apply(event.matches ? 'dark' : 'light')
  }

  apply(system ? system.matches ? 'dark' : 'light' : 'dark')
  // Manual toggles stay local to this session. Keep listening so the next
  // actual system-theme change resumes automatic matching, without new controls.
  system?.addEventListener('change', onSystemChange)
  if (system && getCurrentScope()) {
    onScopeDispose(() => system.removeEventListener('change', onSystemChange))
  }
  return { value, apply, toggle }
}
