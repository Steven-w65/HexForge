import { ref } from 'vue'
import type { ColorTheme } from '../types'

export function useTheme(root: HTMLElement = document.documentElement) {
  const value = ref<ColorTheme>('dark')

  function apply(theme: ColorTheme): void {
    value.value = theme
    root.dataset.theme = theme
  }

  function toggle(): void {
    apply(value.value === 'dark' ? 'light' : 'dark')
  }

  apply('dark')
  return { value, apply, toggle }
}
