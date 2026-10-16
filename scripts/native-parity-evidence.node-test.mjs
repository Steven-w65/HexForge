import test from 'node:test'
import assert from 'node:assert/strict'
import { parseCargoTests, nativeFingerprint, allBehavioralAssertionsHaveNativeEvidence, allRequiredVisualStatesHaveMatchedCaptures } from './native-parity-evidence.mjs'
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { createHash } from 'node:crypto'
import { deflateSync } from 'node:zlib'
import { spawnSync } from 'node:child_process'

const packages = ['hexforge-core', 'hexforge-controller', 'hexforge-native']
const sha = bytes => createHash('sha256').update(bytes).digest('hex')
async function evidenceRoot(t) {
  const root = await mkdtemp(join(tmpdir(), 'hexforge-parity-gate-'))
  t.after(() => rm(root, { recursive: true, force: true }))
  return root
}
async function executionFixture(t) {
  const root = await evidenceRoot(t)
  for (const file of ['Cargo.toml', 'Cargo.lock']) await writeFile(join(root, file), 'controlled input\n')
  const runs = []
  for (const name of packages) {
    const directory = join(root, 'crates', name)
    for (const folder of ['src', 'tests', ...(name === 'hexforge-native' ? ['assets', 'examples'] : [])])
      await mkdir(join(directory, folder), { recursive: true })
    await writeFile(join(directory, 'Cargo.toml'), name)
    if (name === 'hexforge-native') await writeFile(join(directory, 'build.rs'), '')
    const output = `Running tests/contract.rs (target/debug/contract.exe)\ntest preserves_contract ... ok\n`
    const artifact = `${name}.log`
    await writeFile(join(root, artifact), output)
    runs.push({ package: name, command: `cargo test --offline -p ${name}`, exitCode: 0,
      artifact, sha256: sha(output), tests: parseCargoTests(output) })
  }
  const execution = { result: 'passed', fingerprint: await nativeFingerprint(root), runs }
  const ledger = { declaredCases: 1, entries: [{ id: 'one', source: { path: 'one.test.ts', name: 'one' }, status: 'pending', native: null }] }
  return { root, execution, ledger }
}
async function mappedSourceFixture(t) {
  const fixture = await executionFixture(t)
  const file = 'crates/hexforge-native/tests/contract.rs'
  const bytes = Buffer.from('original assertion with its exact newline\r\n')
  await writeFile(join(fixture.root, 'one.test.ts'), bytes)
  await writeFile(join(fixture.root, file), '// controlled executing assertion\n')
  fixture.ledger.entries[0] = { id: 'one', status: 'passed',
    source: { path: 'one.test.ts', name: 'one', hash: sha(bytes) },
    native: { file, test: 'preserves_contract', rationale: 'Same observable contract.',
      evidence: { command: 'cargo test', result: 'passed', artifact: 'hexforge-native.log' } } }
  fixture.execution.fingerprint = await nativeFingerprint(fixture.root)
  return { ...fixture, bytes }
}

test('cutover archives exact reference bytes and keeps evidence verifiable without the frontend', async t => {
  const { root, ledger, execution, bytes } = await mappedSourceFixture(t)
  await writeFile(join(root, 'ledger.json'), JSON.stringify(ledger))
  const script = new URL('./native-parity-evidence.mjs', import.meta.url)
  const archive = 'crates/hexforge-controller/tests/fixtures/reference-assertions.json'
  const output = spawnSync(process.execPath, [script.pathname.replace(/^\/(\w:)/, '$1'),
    '--archive-reference', 'ledger.json', archive], { cwd: root, encoding: 'utf8' })
  assert.equal(output.status, 0, output.stderr)
  const saved = JSON.parse(await readFile(join(root, archive), 'utf8'))
  assert.equal(saved.schema, 1)
  assert.equal(saved.files.length, 1)
  assert.deepEqual(Buffer.from(saved.files[0].bytesBase64, 'base64'), bytes)
  assert.equal(saved.files[0].sha256, sha(bytes))
  await rm(join(root, 'one.test.ts'))
  execution.fingerprint = await nativeFingerprint(root)
  assert.deepEqual(await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root }), [])
  saved.files[0].bytesBase64 = Buffer.from('tampered assertion').toString('base64')
  await writeFile(join(root, archive), JSON.stringify(saved))
  execution.fingerprint = await nativeFingerprint(root)
  assert.ok((await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root }))
    .some(error => error.includes('Source assertion')), 'Archived bytes must still match the original source hash')
})

test('cutover refuses a changed source and never overwrites an earlier reference archive', async t => {
  const { root, ledger } = await mappedSourceFixture(t)
  await writeFile(join(root, 'ledger.json'), JSON.stringify(ledger))
  const script = new URL('./native-parity-evidence.mjs', import.meta.url)
  const args = [script.pathname.replace(/^\/(\w:)/, '$1'), '--archive-reference', 'ledger.json', 'archive.json']
  await writeFile(join(root, 'archive.json'), 'earlier evidence')
  assert.notEqual(spawnSync(process.execPath, args, { cwd: root }).status, 0)
  assert.equal(await readFile(join(root, 'archive.json'), 'utf8'), 'earlier evidence')
  await rm(join(root, 'archive.json'))
  await writeFile(join(root, 'one.test.ts'), 'changed after verification')
  assert.notEqual(spawnSync(process.execPath, args, { cwd: root }).status, 0)
  await assert.rejects(readFile(join(root, 'archive.json')), { code: 'ENOENT' })
})
function png() {
  function chunk(name, data) {
    const out = Buffer.alloc(data.length + 12); out.writeUInt32BE(data.length); out.write(name, 4); data.copy(out, 8)
    let crc = 0xffffffff
    for (const b of out.subarray(4, -4)) { crc ^= b; for (let bit = 0; bit < 8; bit++) crc = crc & 1 ? 0xedb88320 ^ crc >>> 1 : crc >>> 1 }
    out.writeUInt32BE((crc ^ 0xffffffff) >>> 0, out.length - 4)
    return out
  }
  const header = Buffer.alloc(13); header.writeUInt32BE(2); header.writeUInt32BE(1, 4); header[8] = 8; header[9] = 6
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header),
    chunk('IDAT', deflateSync(Buffer.from([0, 201, 209, 217, 255, 17, 20, 24, 255]))), chunk('IEND', Buffer.alloc(0))])
}
async function visualFixture(t) {
  const root = await evidenceRoot(t), captures = []
  for (const theme of ['dark', 'light']) {
    const pair = { scene: 'empty', theme, verified: true,
      comparison: { result: 'passed', artifact: `${theme}-comparison.json`, reviewedDifferences: 'Identical controlled pixels, no masks.' } }
    for (const side of ['reference', 'native']) {
      const file = `${theme}-${side}.png`, metadata = `${theme}-${side}.json`, bytes = png()
      await writeFile(join(root, file), bytes)
      await writeFile(join(root, metadata), JSON.stringify({ evidence: 'lossless', scene: 'empty', theme,
        clientSize: [2, 1], physicalSize: [2, 1], dpr: 1, focusLabel: null,
        state: { file: null, viewportOffset: '0', selection: null, mode: 'fit', bytesPerRow: 16 } }))
      pair[side] = { file, metadata, sha256: sha(bytes) }
    }
    const comparison = { schema: 1, scene: 'empty', theme, result: 'passed',
      reviewedDifferences: pair.comparison.reviewedDifferences,
      pixelSummary: { physicalSize: [2, 1], changedPixels: 0, maximumChannelDelta: 0, differenceBounds: null } }
    for (const side of ['reference', 'native']) comparison[side] = {
      sha256: pair[side].sha256, metadataSha256: sha(await readFile(join(root, pair[side].metadata))) }
    const artifact = JSON.stringify(comparison)
    await writeFile(join(root, pair.comparison.artifact), artifact)
    pair.comparison.sha256 = sha(artifact)
    captures.push(pair)
  }
  return { root, manifest: { captures } }
}

test('executed assertions retain their owning Cargo target, not a same-name count', () => {
  const output = 'Running tests\\policy_parity.rs (target/debug/policy.exe)\ntest every_command_branch_matches_reference ... ok\nRunning tests\\shortcut_parity.rs (target/debug/shortcut.exe)\ntest shortcut_labels_match_window_bindings ... ok\ntest wrong_modifiers ... FAILED\n'
  assert.deepEqual(parseCargoTests(output), [
    { target: 'tests/policy_parity.rs', test: 'every_command_branch_matches_reference', result: 'passed' },
    { target: 'tests/shortcut_parity.rs', test: 'shortcut_labels_match_window_bindings', result: 'passed' },
    { target: 'tests/shortcut_parity.rs', test: 'wrong_modifiers', result: 'failed' },
  ])
  assert.deepEqual(parseCargoTests('test fabricated_name ... ok\n'), [], 'A bare name has no executing target')
})

test('all_behavioral_assertions_have_native_evidence', async () => {
  const ledger = { declaredCases: 1, entries: [{ id: 'source#one#1', source: { path: 'one.test.ts', name: 'one', hash: 'hash' }, status: 'pending', native: null }] }
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, null, { root: '.' })
  assert.ok(errors.some(e => e.includes('Unmapped or unverified')))
  assert.ok(errors.some(e => e.includes('Current native execution')))
})

test('declared passed status cannot substitute for current native execution', async () => {
  const ledger = { declaredCases: 1, entries: [{ id: 'source#one#1', source: { path: 'one.test.ts', name: 'one' }, status: 'passed',
    native: { file: 'crates/hexforge-native/tests/modal_focus.rs', test: 'invented', evidence: { command: 'cargo test', result: 'passed', artifact: 'invented.log' } } }] }
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, { fingerprint: 'old', runs: [] }, { root: '.' })
  assert.ok(errors.some(e => e.includes('Current native execution')))
})

test('all_required_visual_states_have_matched_captures', async () => {
  const errors = await allRequiredVisualStatesHaveMatchedCaptures({ captures: [] }, { root: '.' })
  assert.ok(errors.some(e => e.includes('empty/dark')))
  assert.ok(errors.some(e => e.includes('editor-close/light')))
  assert.ok(errors.some(e => e.includes('proportional-eof/dark')))
})

test('default-theme screenshots and unverified captures never clear the visual gate', async () => {
  const errors = await allRequiredVisualStatesHaveMatchedCaptures({ captures: [{ scene: 'empty', theme: 'dark', verified: false,
    evidence: 'lossless', clientSize: [1280, 800], dpi: 96, file: 'empty.png' }] }, { root: '.' })
  assert.ok(errors.some(e => e.includes('Unverified visual pair: empty/dark')))
  assert.ok(errors.some(e => e.includes('Missing visual pair: empty/light')))
})

test('duplicate package evidence cannot hide an unexecuted native crate', async t => {
  const { root, ledger, execution } = await executionFixture(t)
  execution.runs[2] = execution.runs[1]
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root, requireComplete: false })
  assert.ok(errors.some(e => e.includes('package set')), errors.join('\n'))
})

test('execution provenance rejects a command that did not test its claimed package', async t => {
  const { root, ledger, execution } = await executionFixture(t)
  execution.runs[2].command = 'cargo check -p hexforge-native'
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root, requireComplete: false })
  assert.ok(errors.some(e => e.includes('command differs')), errors.join('\n'))
})

test('changing a vendored native framework invalidates previous execution evidence', async t => {
  const { root, ledger, execution } = await executionFixture(t)
  await mkdir(join(root, 'vendor', 'eframe', 'src'), { recursive: true })
  await writeFile(join(root, 'vendor', 'eframe', 'Cargo.toml'), 'controlled framework manifest\n')
  await writeFile(join(root, 'vendor', 'eframe', 'src', 'lib.rs'), 'controlled original integration\n')
  execution.fingerprint = await nativeFingerprint(root)
  await writeFile(join(root, 'vendor', 'eframe', 'src', 'lib.rs'), 'changed native window integration\n')
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root, requireComplete: false })
  assert.ok(errors.some(e => e.includes('fingerprint differs')), errors.join('\n'))
})

test('changing the invoked portable verifier invalidates previous native execution evidence', async t => {
  const { root, ledger, execution } = await executionFixture(t)
  await mkdir(join(root, 'scripts'))
  const script = join(root, 'scripts', 'verify_portable_native.ps1')
  await writeFile(script, 'original tested verifier\n')
  execution.fingerprint = await nativeFingerprint(root)
  await writeFile(script, 'changed verifier after native tests ran\n')
  const errors = await allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root, requireComplete: false })
  assert.ok(errors.some(e => e.includes('fingerprint differs')), errors.join('\n'))
})

test('execution inventory retains explicitly ignored fixture regeneration with its reason', () => {
  assert.deepEqual(parseCargoTests('Running tests/fixtures.rs (target/debug/fixtures.exe)\n'
    + 'test fixture_regeneration_is_byte_identical ... ok\n'
    + 'test regenerate_bundled_fixtures ... ignored, explicit fixture regeneration writes bundled files\n'), [
    { target: 'tests/fixtures.rs', test: 'fixture_regeneration_is_byte_identical', result: 'passed' },
    { target: 'tests/fixtures.rs', test: 'regenerate_bundled_fixtures', result: 'ignored' },
  ])
})

test('matched lossless captures require a hash-bound comparison that actually exists', async t => {
  const { root, manifest } = await visualFixture(t)
  assert.deepEqual(await allRequiredVisualStatesHaveMatchedCaptures(manifest, { root, scenes: ['empty'] }), [])
  await rm(join(root, manifest.captures[0].comparison.artifact))
  const errors = await allRequiredVisualStatesHaveMatchedCaptures(manifest, { root, scenes: ['empty'] })
  assert.ok(errors.some(e => e.includes('comparison artifact')), errors.join('\n'))
})

test('a visual pair with different actual focus or application state cannot pass', async t => {
  const { root, manifest } = await visualFixture(t)
  const path = join(root, manifest.captures[0].native.metadata)
  const metadata = JSON.parse(await readFile(path, 'utf8'))
  metadata.focusLabel = 'Search byte pattern'; metadata.state.viewportOffset = '16'
  await writeFile(path, JSON.stringify(metadata))
  const errors = await allRequiredVisualStatesHaveMatchedCaptures(manifest, { root, scenes: ['empty'] })
  assert.ok(errors.some(e => e.includes('Focus differs')), errors.join('\n'))
  assert.ok(errors.some(e => e.includes('Application state differs')), errors.join('\n'))
})

test('a comparison summary cannot claim different pixels were measured', async t => {
  const { root, manifest } = await visualFixture(t)
  const pair = manifest.captures[0], path = join(root, pair.comparison.artifact)
  const comparison = JSON.parse(await readFile(path, 'utf8'))
  comparison.pixelSummary.changedPixels = 1
  const artifact = JSON.stringify(comparison)
  await writeFile(path, artifact); pair.comparison.sha256 = sha(artifact)
  const errors = await allRequiredVisualStatesHaveMatchedCaptures(manifest, { root, scenes: ['empty'] })
  assert.ok(errors.some(e => e.includes('pixel summary differs')), errors.join('\n'))
})
