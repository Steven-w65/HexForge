import { describe, expect, it, vi } from 'vitest'
import { createWindowManager } from './windowManager'

describe('Template Editor window manager', () => {
  it('creates only one window for rapid opens and focuses it afterward', async () => {
    const window = { show: vi.fn().mockResolvedValue(undefined), unminimize: vi.fn().mockResolvedValue(undefined), setFocus: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined), destroy: vi.fn().mockResolvedValue(undefined) }
    const create = vi.fn().mockResolvedValue(window)
    const manager = createWindowManager({ find: vi.fn().mockResolvedValue(null), create })
    await Promise.all([manager.openOrFocus(), manager.openOrFocus()])
    expect(create).toHaveBeenCalledOnce()
    await manager.openOrFocus()
    expect(window.setFocus).toHaveBeenCalledTimes(2)
  })

  it('allows a retry after window creation fails', async () => {
    const create = vi.fn().mockRejectedValueOnce(new Error('Creation failed')).mockResolvedValueOnce({
      show: vi.fn(), unminimize: vi.fn(), setFocus: vi.fn(), close: vi.fn(), destroy: vi.fn(),
    })
    const manager = createWindowManager({ find: vi.fn().mockResolvedValue(null), create })
    await expect(manager.openOrFocus()).rejects.toThrow('Creation failed')
    await expect(manager.openOrFocus()).resolves.toBeUndefined()
    expect(create).toHaveBeenCalledTimes(2)
  })

  it('recreates the editor after it has closed', async () => {
    const create = vi.fn().mockImplementation(async () => ({ show: vi.fn(), unminimize: vi.fn(), setFocus: vi.fn(), close: vi.fn(), destroy: vi.fn() }))
    const manager = createWindowManager({ find: vi.fn().mockResolvedValue(null), create })
    await manager.openOrFocus()
    await manager.close()
    await manager.openOrFocus()
    expect(create).toHaveBeenCalledTimes(2)
  })

  it('force-destroys the editor after the main window has approved exit', async () => {
    const window = { show: vi.fn(), unminimize: vi.fn(), setFocus: vi.fn(), close: vi.fn(), destroy: vi.fn() }
    const manager = createWindowManager({ find: vi.fn().mockResolvedValue(window), create: vi.fn() })
    await manager.destroy()
    expect(window.destroy).toHaveBeenCalledOnce()
    expect(window.close).not.toHaveBeenCalled()
  })
})
