import { describe, expect, it } from 'vitest'
import { summarizeLaunches, variantOrder } from './startup-benchmark.mjs'

const report = (paint, interactive) => ({ metrics: {
  firstMeaningfulPaintMs: paint, interactiveMs: interactive, clockOrigin: 'windows-process-creation',
} })

describe('startup benchmark statistics', () => {
  it('alternates comparison order rather than always testing the candidate later', () => {
    expect(variantOrder(['baseline', 'candidate'], 0)).toEqual(['baseline', 'candidate'])
    expect(variantOrder(['baseline', 'candidate'], 1)).toEqual(['candidate', 'baseline'])
  })
  it('reports an even-sample median and nearest-rank p95 without removing slow launches', () => {
    expect(summarizeLaunches([report(160, 161), report(130, 140), report(110, 120), report(170, 180)])).toEqual({
      count: 4, firstMeaningfulPaint: { medianMs: 145, p95Ms: 170, fastestMs: 110 },
      interactive: { medianMs: 150.5, p95Ms: 180, fastestMs: 120 },
    })
  })

  it('does not substitute mount/title readiness or an uncorrelated clock for real paint', () => {
    expect(() => summarizeLaunches([{ nativeReadyMs: 10 }])).toThrow()
    expect(() => summarizeLaunches([{ metrics: { firstMeaningfulPaintMs: 10, interactiveMs: 11, clockOrigin: 'rust-run-entry' } }])).toThrow()
    expect(() => summarizeLaunches([report(NaN, 10)])).toThrow()
    expect(() => summarizeLaunches([report(100, 90)])).toThrow()
    expect(() => summarizeLaunches([])).toThrow()
  })
})
