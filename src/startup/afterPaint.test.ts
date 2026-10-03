import { afterEach, expect, it, vi } from 'vitest'
import { afterFirstPaint } from './afterPaint'

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals() })

it('waits for contentful paint, runs once, and cancels late notifications', () => {
  vi.useFakeTimers()
  const work = vi.fn()
  let notify!: PerformanceObserverCallback
  const disconnect = vi.fn()
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  vi.stubGlobal('PerformanceObserver', class {
    constructor(callback: PerformanceObserverCallback) { notify = callback }
    observe() {}; disconnect = disconnect
  })
  const cancel = afterFirstPaint(work)
  const entry = (name: string) => ({ getEntries: () => [{ name }] }) as unknown as PerformanceObserverEntryList
  notify(entry('first-paint'), {} as PerformanceObserver)
  expect(work).not.toHaveBeenCalled()
  notify(entry('first-contentful-paint'), {} as PerformanceObserver)
  notify(entry('first-contentful-paint'), {} as PerformanceObserver)
  vi.runAllTimers(); cancel()
  expect(work).toHaveBeenCalledOnce()
  expect(disconnect).toHaveBeenCalled()
})

it('cancels the conservative unsupported-engine fallback on unmount', () => {
  vi.useFakeTimers()
  vi.stubGlobal('PerformanceObserver', undefined)
  const work = vi.fn()
  const cancel = afterFirstPaint(work)
  vi.advanceTimersByTime(1499)
  expect(work).not.toHaveBeenCalled()
  cancel(); vi.runAllTimers()
  expect(work).not.toHaveBeenCalled()
})

it('runs after an already reported paint without waiting for another event', () => {
  vi.useFakeTimers()
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([{ name: 'first-contentful-paint' } as PerformanceEntry])
  const work = vi.fn()
  afterFirstPaint(work)
  expect(work).not.toHaveBeenCalled()
  vi.runAllTimers()
  expect(work).toHaveBeenCalledOnce()
})
