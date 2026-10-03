export interface StartupTimings {
  phases: Array<{ name: string; atMs: number }>
  paints?: Array<{ name: string; atMs: number }>
  mainInterfaceAtMs?: number
  clockSamples?: Array<{ documentBeforeMs: number; processAtMs: number; documentAfterMs: number }>
  resources?: Array<{ path: string; initiatorType: string; startMs: number; durationMs: number }>
  measurementError?: string
}

interface PaintEntry { name: string; startTime: number; presentationTime?: number | null }
interface PaintEnvironment {
  interfacePresent?(): boolean
  interfaceVisible(): boolean
  observePaints(callback: (entries: PaintEntry[]) => void): () => void
  readResources(): NonNullable<StartupTimings['resources']>
}

const mainControlSelectors = ['.app-shell[role="application"]', '[data-testid="file-info-bar"]', '.status-bar',
  '[data-row-width="16"]', '[data-row-width="32"]', '[data-action="empty-open"]',
  ...['file', 'edit', 'navigate', 'template', 'view'].map(menu => `[role="menubar"] > [data-menu="${menu}"]`)]

/** DOM-only check before paint: never introduce a forced layout into the measurement. */
function mainInterfacePresent(doc: Document = document): boolean {
  return doc.visibilityState === 'visible' && mainControlSelectors.every(selector => doc.querySelector(selector)?.textContent?.trim())
}

/** Check the actual interface, not a title, mount hook, or splash/placeholder. */
export function mainInterfaceVisible(doc: Document = document): boolean {
  if (!mainInterfacePresent(doc)) return false
  return mainControlSelectors.every(selector => {
    const element = doc.querySelector<HTMLElement>(selector)
    if (!element || !element.textContent?.trim()) return false
    const rect = element.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0 || rect.bottom <= 0 || rect.right <= 0 || rect.top >= innerHeight || rect.left >= innerWidth) return false
    for (let ancestor: HTMLElement | null = element; ancestor; ancestor = ancestor.parentElement) {
      const style = getComputedStyle(ancestor)
      if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false
    }
    return true
  })
}

const browserPaintEnvironment: PaintEnvironment = {
  interfacePresent: () => mainInterfacePresent(),
  interfaceVisible: () => mainInterfaceVisible(),
  observePaints(callback) {
    const observer = new PerformanceObserver(list => callback(list.getEntries()))
    observer.observe({ type: 'paint', buffered: true })
    return () => observer.disconnect()
  },
  readResources: () => (performance.getEntriesByType('resource') as PerformanceResourceTiming[]).slice(0, 128).map(entry => ({
    path: new URL(entry.name, location.href).pathname.slice(0, 255), initiatorType: entry.initiatorType.slice(0, 32),
    startMs: entry.startTime, durationMs: entry.duration,
  })),
}

/** Milestones are relative to document navigation, not to process creation. */
export function createStartupTimings(
  enabled: () => boolean,
  now = () => performance.now(),
  readPaints: () => ArrayLike<PaintEntry> = () => performance.getEntriesByType?.('paint') ?? [],
  environment: PaintEnvironment = browserPaintEnvironment,
) {
  const phases: StartupTimings['phases'] = []
  let paint: Promise<number> | undefined
  let mainInterfaceAtMs: number | undefined
  function mark(name: string): void {
    if (!enabled() || phases.length >= 32 || phases.some(phase => phase.name === name)) return
    phases.push({ name, atMs: now() })
  }
  function report(): StartupTimings | undefined {
    if (!enabled()) return undefined
    // Paint uses the document clock too, but is not a readiness milestone.
    // A missing entry means the browser hasn't reported paint yet, not zero ms.
    const paints = Array.from(readPaints()).filter(entry =>
      ['first-paint', 'first-contentful-paint'].includes(entry.name)).slice(0, 2)
      .map(entry => ({ name: entry.name, atMs: entry.startTime }))
    return { phases: phases.map(phase => ({ ...phase })), ...(paints.length ? { paints } : {}) }
  }

  function beginMainInterface(): void {
    if (!enabled() || paint) return
    mainInterfaceAtMs = now()
    paint = new Promise<number>((resolve, reject) => {
      if (!(environment.interfacePresent ?? environment.interfaceVisible)()) { reject(new Error('The real main controls are not visible.')); return }
      let dispose = () => {}
      const timeout = setTimeout(() => { dispose(); reject(new Error('No verified contentful paint within 5 seconds.')) }, 5000)
      const accept = (entries: PaintEntry[]): void => {
        const entry = entries.find(entry => entry.name === 'first-contentful-paint')
        if (!entry) return
        clearTimeout(timeout); dispose()
        const atMs = entry.presentationTime && entry.presentationTime >= entry.startTime ? entry.presentationTime : entry.startTime
        if (atMs < mainInterfaceAtMs! || !environment.interfaceVisible()) {
          reject(new Error('Contentful paint did not contain the visible main controls.')); return
        }
        resolve(atMs)
      }
      try { dispose = environment.observePaints(accept); accept(Array.from(readPaints())) }
      catch (error) { clearTimeout(timeout); dispose(); reject(error) }
    })
    // Observation begins before listeners finish. Retain its failure until the
    // report is requested, without an unhandled rejection or startup disruption.
    void paint.catch(() => {})
  }

  async function completeReport(sampleClock: () => Promise<{ atMs: number } | null>): Promise<StartupTimings | undefined> {
    if (!enabled()) return undefined
    if (!paint) return report() // A component-only launch cannot claim a paint metric.
    try {
      const atMs = await paint
      const clockSamples: NonNullable<StartupTimings['clockSamples']> = []
      // Calibration occurs AFTER measured paint; choose minimum RTT in Rust.
      // Never subtract a navigation-relative timestamp from Rust's clock raw.
      for (let i = 0; i < 5; i++) {
        const documentBeforeMs = now()
        const sample = await sampleClock()
        const documentAfterMs = now()
        if (!sample || !Number.isFinite(sample.atMs)) throw new Error('Native startup clock unavailable.')
        clockSamples.push({ documentBeforeMs, processAtMs: sample.atMs, documentAfterMs })
      }
      const result = report()!
      result.paints = [{ name: 'first-contentful-paint', atMs }]
      return { ...result, mainInterfaceAtMs, clockSamples, resources: environment.readResources() }
    } catch (error) {
      return { ...report()!, measurementError: (error instanceof Error ? error.message : 'Paint measurement failed.').slice(0, 255) }
    }
  }
  return { mark, report, beginMainInterface, completeReport, enabled }
}

// Injected by Rust only when HEXFORGE_STARTUP_PROFILE names a local output file.
// No timers, logging, storage or extra IPC are used during ordinary launches.
export const startupTimings = createStartupTimings(() =>
  (globalThis as typeof globalThis & { __HEXFORGE_STARTUP_PROFILE__?: boolean }).__HEXFORGE_STARTUP_PROFILE__ === true)
