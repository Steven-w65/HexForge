const OFFSET_LITERAL = /^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/
const BYTE_LITERAL = /^[0-9a-fA-F]{2}$/
const ASCII_WHITESPACE = /[ \t\n\r\f\v]+/

export function parseOffset(text: string, fileSize: bigint): bigint {
  if (typeof text !== 'string' || !OFFSET_LITERAL.test(text)) {
    throw new Error('Offset must be a decimal or 0x-prefixed hexadecimal integer')
  }
  if (fileSize < 0n) {
    throw new RangeError('File size cannot be negative')
  }
  const offset = BigInt(text)
  if (offset < 0n || offset >= fileSize) {
    throw new RangeError('Offset is outside the file')
  }
  return offset
}

export function parseHexBytes(text: string): number[] {
  if (typeof text !== 'string' || text.length === 0) {
    throw new Error('At least one byte is required')
  }
  const normalized = text.replace(/^[ \t\n\r\f\v]+|[ \t\n\r\f\v]+$/g, '')
  if (normalized.length === 0) {
    throw new Error('At least one byte is required')
  }
  const tokens = normalized.split(ASCII_WHITESPACE)
  if (tokens.length === 0 || tokens.some((token) => !BYTE_LITERAL.test(token))) {
    throw new Error('Bytes must be two hexadecimal digits separated by ASCII whitespace')
  }
  return tokens.map((token) => Number.parseInt(token, 16))
}

export function parseSingleByte(text: string): number {
  if (typeof text !== 'string' || !BYTE_LITERAL.test(text)) {
    throw new Error('Byte must be exactly two hexadecimal digits')
  }
  return Number.parseInt(text, 16)
}
