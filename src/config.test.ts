// @vitest-environment node
import { describe, expect, it } from 'vitest'
import type { UserConfig } from 'vite'
import packageJson from '../package.json'
import tauriConfig from '../src-tauri/tauri.conf.json'
import capabilities from '../src-tauri/capabilities/default.json'
import viteConfig from '../vite.config'

describe('desktop development configuration', () => {
  it('pins Vite and points Tauri at the same live development URL', () => {
    const server = (viteConfig as UserConfig).server

    expect(server?.port).toBe(5173)
    expect(server?.strictPort).toBe(true)
    expect(tauriConfig.build.devUrl).toBe('http://localhost:5173')
  })

  it('declares the Node versions supported by the Vite toolchain', () => {
    expect(packageJson.engines.node).toBe('^20.19.0 || >=22.12.0')
  })

  it('builds a portable executable without installer bundles', () => {
    expect(tauriConfig.bundle.active).toBe(false)
  })

  it('allows menu Exit to request a guarded close and the accepted request to destroy the window', () => {
    expect(capabilities.permissions).toContain('core:window:allow-close')
    expect(capabilities.permissions).toContain('core:window:allow-destroy')
  })
})
