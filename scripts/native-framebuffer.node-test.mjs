// Execute the same real renderer as the portable app. Shape/color assertions
// alone cannot catch renderer-wide dithering changing the source's flat colors.
import test from 'node:test'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { readPng } from './parity-png.mjs'

test('real native framebuffer preserves flat source colors without texture-shaped noise', {
  skip: process.platform !== 'win32', timeout: 30_000,
}, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'hexforge-framebuffer-test-'))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const output = join(directory, 'empty.png')
  const fixtureProcess = spawn(resolve('target/debug/examples/capture_parity.exe'),
    ['empty', 'dark', '1280', '800', '1', output], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] })
  let diagnostics = ''
  fixtureProcess.stderr.on('data', data => { diagnostics += data })
  const exit = await new Promise((yes, no) => { fixtureProcess.once('error', no); fixtureProcess.once('exit', yes) })
  assert.equal(exit, 0, diagnostics)
  const frame = readPng(await readFile(output), 1280, 800)
  let wrong = 0
  for (let y = 130; y < 180; y++) for (let x = 200; x < 400; x++) {
    const i = (y * frame.width + x) * 4
    if (frame.rgba[i] !== 17 || frame.rgba[i + 1] !== 20 || frame.rgba[i + 2] !== 24 || frame.rgba[i + 3] !== 255) wrong++
  }
  assert.equal(wrong, 0, 'the unobstructed --bg region must remain #111418, not gain renderer noise')
})
