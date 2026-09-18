import { describe, expect, it } from 'vitest'
import { parseHexBytes, parseOffset, parseSingleByte } from './input'

describe('hex input parsing', () => {
  it.each([
    ['42', 42n],
    ['0x2A', 42n],
    ['0X2a', 42n],
  ])('parses an anchored offset literal: %s', (text, expected) => {
    expect(parseOffset(text, 100n)).toBe(expected)
  })

  it.each(['', '   ', '-1', '+1', '0x', '0x2g', '12x', '100'])(
    'rejects invalid or out-of-range offset: %s',
    (text) => {
      expect(() => parseOffset(text, 100n)).toThrow()
    },
  )

  it('rejects every offset for an empty file', () => {
    expect(() => parseOffset('0', 0n)).toThrow()
  })

  it('keeps offsets exact beyond Number safe integer precision', () => {
    const offset = 9007199254740993n
    expect(parseOffset(offset.toString(), offset + 1n)).toBe(offset)
    expect(parseOffset(`0x${offset.toString(16)}`, offset + 1n)).toBe(offset)
  })

  it('parses complete byte tokens and preserves sequence', () => {
    expect(parseHexBytes('41 42\tff\n00')).toEqual([0x41, 0x42, 0xff, 0])
    expect(parseHexBytes('  41\n42  ')).toEqual([0x41, 0x42])
  })

  it.each(['', '   ', '4', 'GG', '0', '000', '41-42'])('rejects malformed byte input: %s', (text) => {
    expect(() => parseHexBytes(text)).toThrow()
  })

  it.each([
    ['00', 0],
    ['aF', 0xaf],
    ['FF', 255],
  ])('parses one edit byte: %s', (text, expected) => {
    expect(parseSingleByte(text)).toBe(expected)
  })

  it.each(['', 'F', 'FFF', 'GG', '0xFF', ' FF'])('rejects malformed edit byte: %s', (text) => {
    expect(() => parseSingleByte(text)).toThrow()
  })
})
