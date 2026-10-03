let pending: Promise<typeof import('../components/HexCanvas.vue')['default']> | undefined

/** Opening a file and post-paint prewarm share a single request. A failed
 * optional prewarm is retryable; it never prevents opening the actual viewer.
 */
export function loadHexCanvas() {
  return pending ??= import('../components/HexCanvas.vue').then(module => module.default).catch(error => {
    pending = undefined
    throw error
  })
}
