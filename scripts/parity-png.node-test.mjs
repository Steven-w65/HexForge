// Original lossless-evidence assertions retained independently of browser tooling.
import test from 'node:test'
import assert from 'node:assert/strict'
import { deflateSync } from 'node:zlib'
import { verifyPng, readPng } from './parity-png.mjs'

function crc(bytes) { let c = 0xffffffff; for (const b of bytes) { c ^= b; for (let bit = 0; bit < 8; bit++) c = c & 1 ? 0xedb88320 ^ c >>> 1 : c >>> 1 }; return (c ^ 0xffffffff) >>> 0 }
function chunk(name, data) { const out = Buffer.alloc(data.length + 12); out.writeUInt32BE(data.length); out.write(name, 4); data.copy(out, 8); out.writeUInt32BE(crc(out.subarray(4, -4)), out.length - 4); return out }
function png(scanlines = [0, 201, 209, 217, 255, 17, 20, 24, 255], width = 2, height = 1) {
  const header = Buffer.alloc(13); header.writeUInt32BE(width); header.writeUInt32BE(height, 4); header[8] = 8; header[9] = 6
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header),
    chunk('IDAT', deflateSync(Buffer.from(scanlines))), chunk('IEND', Buffer.alloc(0))])
}
test('lossless capture validates real PNG signature and exact physical client dimensions', () => {
  const bytes = png()
  assert.doesNotThrow(() => verifyPng(bytes, 2, 1))
  assert.throws(() => verifyPng(bytes, 1282, 831), /rescaling/)
  assert.throws(() => verifyPng(Buffer.from([255, 216, 255]), 1280, 800), /PNG/)
})

test('a PNG header without an actual lossless framebuffer never counts as pixel evidence', () => {
  assert.throws(() => verifyPng(png().subarray(0, 24), 2, 1), /PNG/)
})

test('corrupted, trailing and incomplete PNG data cannot clear a visual gate', () => {
  const corrupt = png(); corrupt[50] ^= 1
  for (const bad of [corrupt, png().subarray(0, -12), Buffer.concat([png(), Buffer.from([0])])]) {
    assert.throws(() => verifyPng(bad, 2, 1), /PNG/)
  }
})

test('pixel comparison decodes all PNG row filters without resampling glyphs', () => {
  const bytes = png([0, 10, 20, 30, 255, 1, 20, 30, 40, 255, 2, 10, 10, 10, 0,
    3, 25, 30, 35, 128, 4, 10, 10, 10, 0], 1, 5)
  const decoded = readPng(bytes)
  assert.deepEqual([...decoded.rgba], [10, 20, 30, 255, 20, 30, 40, 255,
    30, 40, 50, 255, 40, 50, 60, 255, 50, 60, 70, 255])
  assert.throws(() => verifyPng(png([5, 0, 0, 0, 0], 1, 1), 1, 1), /PNG/)
  assert.throws(() => verifyPng(png([0, 0], 1, 1), 1, 1), /PNG/)
})
