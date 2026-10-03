import { describe, expect, it, vi } from 'vitest'
import { createStartupTimings } from './timings'

describe('opt-in startup phase timings', () => {
  it('does no clock work and produces no report when disabled', () => {
    const profile = createStartupTimings(() => false, () => { throw new Error('Must stay idle') })
    profile.mark('main-mounted')
    expect(profile.report()).toBeUndefined()
  })

  it('records document-relative milestones once and returns independent snapshots', () => {
    let now = 45
    const profile = createStartupTimings(() => true, () => now)
    profile.mark('entry')
    now = 90; profile.mark('root-loaded')
    now = 120; profile.mark('root-loaded')
    const report = profile.report()!
    expect(report.phases).toEqual([{ name: 'entry', atMs: 45 }, { name: 'root-loaded', atMs: 90 }])
    report.phases[0]!.atMs = 999
    expect(profile.report()!.phases[0]!.atMs).toBe(45)
  })

  it('bounds the diagnostic payload even if repeatedly called', () => {
    const profile = createStartupTimings(() => true, () => 1)
    for (let i = 0; i < 100; i++) profile.mark(`phase-${i}`)
    expect(profile.report()!.phases).toHaveLength(32)
  })

  it('reports browser paint timestamps separately without changing readiness milestones', () => {
    const readPaints = vi.fn(() => [
      { name: 'first-paint', startTime: 75 },
      { name: 'first-contentful-paint', startTime: 80 },
      { name: 'unrelated', startTime: 90 },
    ])
    const profile = createStartupTimings(() => true, () => 100, readPaints)
    profile.mark('listeners-ready')
    expect(profile.report()).toEqual({
      phases: [{ name: 'listeners-ready', atMs: 100 }],
      paints: [{ name: 'first-paint', atMs: 75 }, { name: 'first-contentful-paint', atMs: 80 }],
    })
    const disabled = createStartupTimings(() => false, () => 0, readPaints)
    disabled.report()
    expect(readPaints).toHaveBeenCalledOnce()
  })

  it('waits for contentful paint after the real controls rather than claiming mount as paint', async () => {
    let onPaint!: (entries: Array<{ name: string; startTime: number }>) => void
    let now = 150
    const observe = vi.fn(callback => { onPaint = callback; return vi.fn() })
    const profile = createStartupTimings(() => true, () => now, () => [], {
      interfaceVisible: () => true, observePaints: observe, readResources: () => [],
    })
    profile.beginMainInterface()
    now = 165; profile.mark('listeners-ready')
    const clock = vi.fn(async () => ({ atMs: 400 }))
    const complete = profile.completeReport(clock)
    await Promise.resolve()
    expect(clock).not.toHaveBeenCalled()
    onPaint([{ name: 'first-contentful-paint', startTime: 160 }])
    const report = await complete
    expect(report?.mainInterfaceAtMs).toBe(150)
    expect(report?.paints).toEqual([{ name: 'first-contentful-paint', atMs: 160 }])
    expect(report?.clockSamples).toHaveLength(5)
  })

  it('does not start paint observation or clock IPC in ordinary launches', async () => {
    const observe = vi.fn()
    const profile = createStartupTimings(() => false, () => { throw new Error('Clock must stay idle') }, () => [], {
      interfaceVisible: () => { throw new Error('Layout must stay idle') }, observePaints: observe, readResources: () => [],
    })
    profile.beginMainInterface()
    const clock = vi.fn()
    expect(await profile.completeReport(clock)).toBeUndefined()
    expect(observe).not.toHaveBeenCalled()
    expect(clock).not.toHaveBeenCalled()
  })

  it('does not force layout to validate visibility before the measured paint', async () => {
    let onPaint!: (entries: Array<{ name: string; startTime: number }>) => void
    const geometry = vi.fn(() => true)
    const profile = createStartupTimings(() => true, () => 150, () => [], {
      interfacePresent: () => true, interfaceVisible: geometry,
      observePaints: callback => { onPaint = callback; return () => {} }, readResources: () => [],
    })
    profile.beginMainInterface()
    expect(geometry).not.toHaveBeenCalled()
    profile.mark('listeners-ready')
    onPaint([{ name: 'first-contentful-paint', startTime: 160 }])
    expect((await profile.completeReport(async () => ({ atMs: 400 })))?.measurementError).toBeUndefined()
    expect(geometry).toHaveBeenCalledOnce()
  })
})
