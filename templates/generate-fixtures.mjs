// Regenerate deterministic fixtures with `node templates/generate-fixtures.mjs`.
// Optional arguments select stems, e.g. `node templates/generate-fixtures.mjs bmp-header`.
// Fixtures are actual small files; templates decode only their documented portions.
import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { deflateSync, gzipSync } from 'node:zlib'

const here = dirname(fileURLToPath(import.meta.url))
const stems = [
  'png-ihdr', 'riff-wav', 'elf64-header', 'bmp-header', 'gif-header', 'ico-directory',
  'dds-header', 'zip-local-header', 'gzip-header', 'sqlite-header',
  'pcap-little-header', 'pcap-big-header', 'glb-header', 'uf2-block',
]
const requested = new Set(process.argv.slice(2))
for (const stem of requested) {
  if (!stems.includes(stem)) throw new Error(`Unknown fixture: ${stem}`)
}
const saveFixture = (stem, bytes) => {
  if (requested.size === 0 || requested.has(stem)) writeFileSync(join(here, `${stem}.bin`), bytes)
}
const u32be = (value) => { const out = Buffer.alloc(4); out.writeUInt32BE(value); return out }
const crc32 = (bytes) => {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0)
  }
  return (crc ^ 0xffffffff) >>> 0
}
const pngChunk = (name, data) => {
  const content = Buffer.concat([Buffer.from(name, 'ascii'), data])
  return Buffer.concat([u32be(data.length), content, u32be(crc32(content))])
}
const ihdr = Buffer.alloc(13)
ihdr.writeUInt32BE(1, 0) // width
ihdr.writeUInt32BE(1, 4) // height
ihdr[8] = 8 // bit depth
ihdr[9] = 6 // RGBA
const png = Buffer.concat([
  Buffer.from('89504e470d0a1a0a', 'hex'),
  pngChunk('IHDR', ihdr),
  pngChunk('IDAT', deflateSync(Buffer.from([0, 255, 0, 0, 255]))), // filter 0; red RGBA pixel
  pngChunk('IEND', Buffer.alloc(0)),
])
saveFixture('png-ihdr', png)

const wav = Buffer.alloc(48)
wav.write('RIFF', 0, 'ascii')
wav.writeUInt32LE(wav.length - 8, 4)
wav.write('WAVEfmt ', 8, 'ascii')
wav.writeUInt32LE(16, 16) // PCM fmt chunk size
wav.writeUInt16LE(1, 20) // PCM
wav.writeUInt16LE(2, 22) // stereo
wav.writeUInt32LE(8000, 24) // sample rate
wav.writeUInt32LE(32000, 28) // 8000 * 2 channels * 2 bytes
wav.writeUInt16LE(4, 32)
wav.writeUInt16LE(16, 34)
wav.write('data', 36, 'ascii')
wav.writeUInt32LE(4, 40)
wav.writeInt16LE(1000, 44)
wav.writeInt16LE(-1000, 46)
saveFixture('riff-wav', wav)

const elf = Buffer.alloc(64)
Buffer.from('7f454c46', 'hex').copy(elf, 0)
elf[4] = 2 // ELF64
elf[5] = 1 // little endian
elf[6] = 1 // ELF version
elf.writeUInt16LE(2, 16) // executable
elf.writeUInt16LE(62, 18) // x86-64
elf.writeUInt32LE(1, 20)
elf.writeBigUInt64LE(0x401000n, 24) // entry point
elf.writeUInt16LE(64, 52) // e_ehsize
elf.writeUInt16LE(64, 54) // program header entry size (no entries in this fixture)
elf.writeUInt16LE(64, 58) // section header entry size (no entries in this fixture)
saveFixture('elf64-header', elf)

// 1 x 1, bottom-up 24-bit BMP. Each pixel row is padded to a four-byte boundary.
const bmp = Buffer.alloc(58)
bmp.write('BM', 0, 'ascii')
bmp.writeUInt32LE(bmp.length, 2)
bmp.writeUInt32LE(54, 10)
bmp.writeUInt32LE(40, 14)
bmp.writeInt32LE(1, 18)
bmp.writeInt32LE(1, 22)
bmp.writeUInt16LE(1, 26)
bmp.writeUInt16LE(24, 28)
bmp.writeUInt32LE(4, 34)
bmp.writeInt32LE(2835, 38)
bmp.writeInt32LE(2835, 42)
bmp[56] = 255 // BGR pixel: red.
saveFixture('bmp-header', bmp)

// GIF89a with a two-color global palette, one red pixel, and an LZW end code.
const gifHeader = Buffer.alloc(13)
gifHeader.write('GIF89a', 0, 'ascii')
gifHeader.writeUInt16LE(1, 6)
gifHeader.writeUInt16LE(1, 8)
gifHeader[10] = 0x80 // Global color table present, two entries.
saveFixture('gif-header', Buffer.concat([
  gifHeader, Buffer.from([255, 0, 0, 0, 0, 0]),
  Buffer.from([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]),
  Buffer.from([2, 2, 0x44, 0x01, 0, 0x3b]),
]))

// ICO directory with one embedded complete PNG. Offset 22 = 6 + one 16-byte entry.
const ico = Buffer.alloc(22)
ico.writeUInt16LE(1, 2)
ico.writeUInt16LE(1, 4)
ico[6] = ico[7] = 1
ico.writeUInt16LE(1, 10)
ico.writeUInt16LE(32, 12)
ico.writeUInt32LE(png.length, 14)
ico.writeUInt32LE(22, 18)
saveFixture('ico-directory', Buffer.concat([ico, png]))

// Legacy DDS with a single uncompressed A8R8G8B8 red pixel and no mip chain.
const dds = Buffer.alloc(132)
dds.write('DDS ', 0, 'ascii')
dds.writeUInt32LE(124, 4)
dds.writeUInt32LE(0x100f, 8) // Caps, height, width, pitch, pixel format.
dds.writeUInt32LE(1, 12)
dds.writeUInt32LE(1, 16)
dds.writeUInt32LE(4, 20)
dds.writeUInt32LE(32, 76)
dds.writeUInt32LE(0x41, 80) // RGB + alpha pixels.
dds.writeUInt32LE(32, 88)
dds.writeUInt32LE(0x00ff0000, 92)
dds.writeUInt32LE(0x0000ff00, 96)
dds.writeUInt32LE(0x000000ff, 100)
dds.writeUInt32LE(0xff000000, 104)
dds.writeUInt32LE(0x1000, 108) // Texture cap.
dds.writeUInt32LE(0xffff0000, 128)
saveFixture('dds-header', dds)

// A complete stored ZIP archive containing hello.txt, with a central directory.
const name = Buffer.from('hello.txt', 'utf8')
const text = Buffer.from('HexForge\n', 'utf8')
const checksum = crc32(text)
const local = Buffer.alloc(30)
local.writeUInt32LE(0x04034b50, 0)
local.writeUInt16LE(20, 4)
local.writeUInt16LE(0x0800, 6) // UTF-8 filename flag.
local.writeUInt16LE(0x0021, 12) // DOS date: 1980-01-01; time remains 00:00.
local.writeUInt32LE(checksum, 14)
local.writeUInt32LE(text.length, 18)
local.writeUInt32LE(text.length, 22)
local.writeUInt16LE(name.length, 26)
const central = Buffer.alloc(46)
central.writeUInt32LE(0x02014b50, 0)
central.writeUInt16LE(20, 4)
central.writeUInt16LE(20, 6)
central.writeUInt16LE(0x0800, 8)
central.writeUInt16LE(0x0021, 14)
central.writeUInt32LE(checksum, 16)
central.writeUInt32LE(text.length, 20)
central.writeUInt32LE(text.length, 24)
central.writeUInt16LE(name.length, 28)
const zipEnd = Buffer.alloc(22)
zipEnd.writeUInt32LE(0x06054b50, 0)
zipEnd.writeUInt16LE(1, 8)
zipEnd.writeUInt16LE(1, 10)
zipEnd.writeUInt32LE(central.length + name.length, 12)
zipEnd.writeUInt32LE(local.length + name.length + text.length, 16)
saveFixture('zip-local-header', Buffer.concat([local, name, text, central, name, zipEnd]))

// One GZIP member with no optional fields. Normalize the OS byte for every host.
const gzip = gzipSync(text, { level: 9 })
gzip[9] = 255
saveFixture('gzip-header', gzip)

// An empty, valid one-page SQLite database with a leaf-table b-tree on page 1.
const sqlite = Buffer.alloc(512)
sqlite.write('SQLite format 3\0', 0, 'ascii')
sqlite.writeUInt16BE(512, 16)
sqlite[18] = sqlite[19] = 1 // Rollback-journal read/write format.
sqlite[21] = 64
sqlite[22] = sqlite[23] = 32
sqlite.writeUInt32BE(1, 24)
sqlite.writeUInt32BE(1, 28)
sqlite.writeUInt32BE(4, 44)
sqlite.writeUInt32BE(1, 56) // UTF-8.
sqlite.writeUInt32BE(1, 92)
sqlite.writeUInt32BE(3046000, 96)
sqlite[100] = 0x0d // Table leaf: no cells, freeblock, or fragmented bytes.
sqlite.writeUInt16BE(512, 105)
saveFixture('sqlite-header', sqlite)

// Classic PCAP in each byte order, with one 60-byte experimental Ethernet frame.
for (const order of ['little', 'big']) {
  const endian = order === 'little' ? 'LE' : 'BE'
  const fileHeader = Buffer.alloc(24)
  fileHeader[`writeUInt32${endian}`](0xa1b2c3d4, 0)
  fileHeader[`writeUInt16${endian}`](2, 4)
  fileHeader[`writeUInt16${endian}`](4, 6)
  fileHeader[`writeUInt32${endian}`](65535, 16)
  fileHeader[`writeUInt32${endian}`](1, 20) // Ethernet.
  const packet = Buffer.alloc(60)
  packet.fill(255, 0, 6) // Broadcast destination.
  packet[6] = 2 // Locally administered source address.
  packet[11] = 1
  packet.writeUInt16BE(0x88b5, 12) // Local experimental EtherType.
  text.copy(packet, 14)
  const packetHeader = Buffer.alloc(16)
  packetHeader[`writeUInt32${endian}`](1700000000, 0)
  packetHeader[`writeUInt32${endian}`](123456, 4)
  packetHeader[`writeUInt32${endian}`](packet.length, 8)
  packetHeader[`writeUInt32${endian}`](packet.length, 12)
  saveFixture(`pcap-${order}-header`, Buffer.concat([fileHeader, packetHeader, packet]))
}

// A minimal GLB 2 scene with one JSON chunk and four-byte space padding.
const gltf = Buffer.from(JSON.stringify({ asset: { version: '2.0' }, scene: 0, scenes: [{}] }), 'utf8')
const jsonChunk = Buffer.alloc((gltf.length + 3) & ~3, 0x20)
gltf.copy(jsonChunk)
const glb = Buffer.alloc(20)
glb.write('glTF', 0, 'ascii')
glb.writeUInt32LE(2, 4)
glb.writeUInt32LE(glb.length + jsonChunk.length, 8)
glb.writeUInt32LE(jsonChunk.length, 12)
glb.write('JSON', 16, 'ascii')
saveFixture('glb-header', Buffer.concat([glb, jsonChunk]))

// One UF2 data block with a 256-byte payload; no family ID or extension flags.
const uf2 = Buffer.alloc(512)
uf2.writeUInt32LE(0x0a324655, 0)
uf2.writeUInt32LE(0x9e5d5157, 4)
uf2.writeUInt32LE(0x10002000, 12)
uf2.writeUInt32LE(256, 16)
uf2.writeUInt32LE(1, 24)
Buffer.from('deadbeef', 'hex').copy(uf2, 32)
uf2.writeUInt32LE(0x0ab16f30, 508)
saveFixture('uf2-block', uf2)
