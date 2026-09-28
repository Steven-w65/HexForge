import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve, sep } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { createServer } from 'vite'

if (process.platform !== 'win32') throw new Error('This real-Canvas test uses the installed Windows Edge browser.')

const edge = [
  process.env.EDGE_PATH,
  join(process.env['PROGRAMFILES(X86)'] ?? 'C:\\Program Files (x86)', 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
  join(process.env.PROGRAMFILES ?? 'C:\\Program Files', 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
].find((candidate) => candidate && existsSync(candidate))
if (!edge) throw new Error('Microsoft Edge is required for the real-Canvas regression test.')

async function until(probe, description, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const result = await probe()
    if (result) return result
    await delay(100)
  }
  throw new Error(`Timed out waiting for ${description}.`)
}

async function connect(wsUrl) {
  const socket = new WebSocket(wsUrl)
  await new Promise((accept, reject) => {
    socket.addEventListener('open', accept, { once: true })
    socket.addEventListener('error', reject, { once: true })
  })
  let nextId = 0
  const pending = new Map()
  socket.addEventListener('message', (event) => {
    const response = JSON.parse(event.data)
    const request = pending.get(response.id)
    if (!request) return
    pending.delete(response.id)
    if (response.error) request.reject(new Error(response.error.message))
    else request.resolve(response.result)
  })
  socket.addEventListener('close', () => {
    for (const request of pending.values()) request.reject(new Error('Edge debugging connection closed.'))
    pending.clear()
  })
  return {
    socket,
    call(method, params = {}) {
      const id = ++nextId
      return new Promise((resolveCall, reject) => {
        const timeout = setTimeout(() => {
          pending.delete(id)
          reject(new Error(`Timed out waiting for Edge ${method}.`))
        }, 30_000)
        pending.set(id, {
          resolve(value) { clearTimeout(timeout); resolveCall(value) },
          reject(error) { clearTimeout(timeout); reject(error) },
        })
        try { socket.send(JSON.stringify({ id, method, params })) }
        catch (error) { pending.get(id)?.reject(error); pending.delete(id) }
      })
    },
  }
}

let server
let browser
let connection
let profile
try {
  server = await createServer({ logLevel: 'error', optimizeDeps: { entries: ['tests/visual/minimap.html'] },
    server: { host: '127.0.0.1', port: 0 } })
  await server.listen()
  const address = server.httpServer.address()
  assert(address && typeof address !== 'string')
  const url = `http://127.0.0.1:${address.port}/tests/visual/minimap.html`
  profile = await mkdtemp(join(tmpdir(), 'hexforge-real-canvas-'))
  browser = spawn(edge, [
    '--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
    '--disable-extensions', '--remote-debugging-port=0', `--user-data-dir=${profile}`, url,
  ], { stdio: 'ignore', windowsHide: true })

  const port = await until(async () => {
    try { return Number((await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]) || null }
    catch { return null }
  }, 'Edge debugging port')
  const tab = await until(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2000) })
      const tabs = await response.json()
      return tabs.find((item) => item.type === 'page' && item.url.includes('/tests/visual/minimap.html'))
    } catch { return null }
  }, 'minimap visual test page')
  connection = await connect(tab.webSocketDebuggerUrl)

  const result = await until(async () => {
    const evaluation = await connection.call('Runtime.evaluate', {
      expression: '({ result: window.__hexforgeVisualResults, error: window.__hexforgeVisualError })', returnByValue: true,
    })
    const value = evaluation.result?.value
    if (value?.error) throw new Error(`Visual fixture failed: ${value.error}`)
    return value?.result ?? null
  }, 'real Canvas results', 45_000)

  assert.equal(result.fit16.bottomRowPainted, true, 'fit 16-byte preview must paint its bottom row')
  assert.equal(result.fit32.bottomRowPainted, true, 'fit 32-byte preview must paint its bottom row')
  assert.equal(result.proportional16.bottomRowPainted, true, 'proportional 16-byte preview must show EOF')
  assert.equal(result.proportional32.bottomRowPainted, true, 'proportional 32-byte preview must show EOF')
  assert.equal(result.proportional32.lastColumnPainted, true, '32-byte preview must render the final column')
  assert.equal(result.proportional32.boxBorderPainted, true, 'viewport box must remain visible over the preview')
  for (const item of [result.fit16, result.fit32, result.proportional16, result.proportional32]) {
    assert(item.backingHeight <= 100, 'backing Canvas exceeded viewport height')
  }
  for (const item of [result.component16Fit, result.component32Proportional]) {
    assert.equal(item.addressGutterPainted, true, 'opening a file must paint the fixed address gutter')
    assert.equal(item.hexEndsBeforeMinimap, true, 'hex text must not overlap the minimap column')
    assert.equal(item.minimapFillsHeight, true, 'long-file minimap must fill the editor viewport')
  }
  assert.equal(result.templateDialogButtons.borderStyle, 'none', 'Template Editor close actions must not show native button borders')
  assert.equal(result.templateDialogButtons.borderRadius, '3px', 'Template Editor close actions must match editor button rounding')
  assert.equal(result.templateDialogButtons.height, '27px', 'Template Editor close actions must match editor button height')
  assert.equal(result.templateDialogButtons.disabledOpacity, '0.4', 'disabled Save must match editor action opacity')
  console.log('Verified real Canvas, HexCanvas, and Template Editor dialog styling in Edge.')
} finally {
  connection?.socket.close()
  if (browser && !browser.killed) browser.kill()
  await server?.close()
  if (profile) {
    const target = resolve(profile)
    const tempRoot = resolve(tmpdir()) + sep
    if (target.startsWith(tempRoot)) await rm(target, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 })
  }
}
