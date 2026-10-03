/** Schedule optional work after the browser has painted, not after Vue mount.
 * A conservative timer fallback keeps functionality on engines without Paint Timing.
 * It is not a readiness signal and is never used to claim a measured paint.
 */
export function afterFirstPaint(work: () => void): () => void {
  let active = true
  let observer: PerformanceObserver | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  const cancel = () => { active = false; observer?.disconnect(); if (timer !== undefined) clearTimeout(timer) }
  const run = () => { if (!active) return; cancel(); work() }
  const hasPaint = (entries: ArrayLike<PerformanceEntry>) => Array.from(entries).some(entry => entry.name === 'first-contentful-paint')
  try {
    if (hasPaint(performance.getEntriesByType('paint'))) {
      timer = setTimeout(run, 0)
    } else {
      observer = new PerformanceObserver(list => { if (hasPaint(list.getEntries())) run() })
      observer.observe({ type: 'paint', buffered: true })
      timer = setTimeout(run, 1500)
    }
  } catch { timer = setTimeout(run, 1500) }
  return cancel
}
