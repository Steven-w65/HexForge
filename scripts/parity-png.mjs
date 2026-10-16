// Development evidence only: decode actual bounded PNG pixels, never resize.
// Both native image::png and Chromium framebuffer PNGs use noninterlaced RGB/A.
import { inflateSync } from 'node:zlib'

const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])
const crcTable = Uint32Array.from({ length: 256 }, (_, n) => {
  for (let bit = 0; bit < 8; bit++) n = n & 1 ? 0xedb88320 ^ n >>> 1 : n >>> 1
  return n >>> 0
})
function crc(bytes) {
  let n = 0xffffffff
  for (const b of bytes) n = crcTable[(n ^ b) & 255] ^ n >>> 8
  return (n ^ 0xffffffff) >>> 0
}
function paeth(a, b, c) {
  const p = a + b - c, da = Math.abs(p - a), db = Math.abs(p - b), dc = Math.abs(p - c)
  return da <= db && da <= dc ? a : db <= dc ? b : c
}
export function readPng(bytes, expectedWidth, expectedHeight) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 33 || bytes.length > 128 * 1024 * 1024 || !bytes.subarray(0, 8).equals(signature)) throw new Error('Lossless bounded PNG framebuffer required')
  let cursor = 8, width, height, channels, ended = false, finishedData = false
  const data = []
  while (cursor < bytes.length) {
    if (bytes.length - cursor < 12) throw new Error('Incomplete PNG chunk')
    const length = bytes.readUInt32BE(cursor), end = cursor + length + 12
    if (end > bytes.length) throw new Error('Incomplete PNG chunk data')
    const name = bytes.toString('ascii', cursor + 4, cursor + 8), value = bytes.subarray(cursor + 8, end - 4)
    if (crc(bytes.subarray(cursor + 4, end - 4)) !== bytes.readUInt32BE(end - 4)) throw new Error('Corrupted PNG checksum')
    if (cursor === 8 && name !== 'IHDR') throw new Error('PNG header must be first')
    if (name === 'IHDR') {
      if (cursor !== 8 || length !== 13) throw new Error('Invalid PNG header')
      width = value.readUInt32BE(0); height = value.readUInt32BE(4)
      if (!width || !height || width > 16_384 || height > 16_384 || width * height > 32_000_000) throw new Error('Unbounded PNG framebuffer')
      if (expectedWidth !== undefined && (width !== expectedWidth || height !== expectedHeight)) throw new Error('PNG client dimensions differ; rescaling is forbidden')
      if (value[8] !== 8 || ![2, 6].includes(value[9]) || value[10] || value[11] || value[12]) throw new Error('Unsupported PNG framebuffer encoding')
      channels = value[9] === 6 ? 4 : 3
    } else if (name === 'IDAT') {
      if (finishedData) throw new Error('Noncontiguous PNG pixel data')
      data.push(value)
    } else if (name === 'IEND') {
      if (length || !data.length || end !== bytes.length) throw new Error('Incomplete or trailing PNG data')
      ended = true
    } else {
      if (data.length) finishedData = true
      if (name[0] === name[0].toUpperCase() && name !== 'PLTE') throw new Error('Unsupported critical PNG chunk')
    }
    cursor = end
  }
  if (!ended) throw new Error('PNG end chunk missing')
  const stride = width * channels, scanlineLength = (stride + 1) * height
  let scanlines
  try { scanlines = inflateSync(Buffer.concat(data), { maxOutputLength: scanlineLength }) }
  catch { throw new Error('Invalid or unbounded PNG compressed pixels') }
  if (scanlines.length !== scanlineLength) throw new Error('Incomplete PNG pixels')
  const pixels = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y++) {
    const start = y * (stride + 1), filter = scanlines[start], row = y * stride
    if (filter > 4) throw new Error('Invalid PNG row filter')
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? pixels[row + x - channels] : 0
      const b = y ? pixels[row + x - stride] : 0
      const c = y && x >= channels ? pixels[row + x - stride - channels] : 0
      const prediction = [0, a, b, Math.floor((a + b) / 2), paeth(a, b, c)][filter]
      pixels[row + x] = (scanlines[start + x + 1] + prediction) & 255
    }
  }
  if (channels === 4) return { width, height, rgba: pixels }
  const rgba = Buffer.alloc(width * height * 4)
  for (let i = 0; i < width * height; i++) {
    pixels.copy(rgba, i * 4, i * 3, i * 3 + 3); rgba[i * 4 + 3] = 255
  }
  return { width, height, rgba }
}
export function verifyPng(bytes, width, height) { readPng(bytes, width, height) }
