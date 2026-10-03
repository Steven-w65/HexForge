import { readFile } from 'node:fs/promises'
import { expect, it } from 'vitest'

it('preloads the real bundled UI font so initial text does not wait for first layout', async () => {
  const html = await readFile('index.html', 'utf8')
  expect(html).toMatch(/<link\s+rel="preload"\s+href="\/src\/assets\/fonts\/JetBrainsMono\.woff2"\s+as="font"\s+type="font\/woff2"\s+crossorigin/)
})
