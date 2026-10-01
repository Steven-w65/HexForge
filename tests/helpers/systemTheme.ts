import { vi } from 'vitest'

/** Native matchMedia boundary double; listeners use the real DOM EventTarget. */
export function installSystemTheme(initialDark: boolean) {
  let dark = initialDark
  const media = '(prefers-color-scheme: dark)'
  const query = new EventTarget() as MediaQueryList
  Object.defineProperties(query, {
    media: { value: media },
    matches: { get: () => dark },
  })
  query.onchange = null
  query.addListener = listener => query.addEventListener('change', listener as EventListener)
  query.removeListener = listener => query.removeEventListener('change', listener as EventListener)
  vi.stubGlobal('matchMedia', (requested: string) => {
    if (requested !== media) throw new Error(`Unexpected media query: ${requested}`)
    return query
  })

  return {
    setDark(value: boolean): void {
      if (dark === value) return
      dark = value
      const event = Object.assign(new Event('change'), { matches: dark, media }) as MediaQueryListEvent
      query.dispatchEvent(event)
      query.onchange?.call(query, event)
    },
  }
}
