// Completion gates are separate from inventory/schema checks. A declared
// "passed" string, test count, screenshot filename or mount signal is no proof.
import { readFile, readdir, mkdir, writeFile, stat } from 'node:fs/promises'
import { resolve, relative, dirname } from 'node:path'
import { createHash } from 'node:crypto'
import { spawn } from 'node:child_process'
import { pathToFileURL } from 'node:url'
import { isDeepStrictEqual } from 'node:util'
import { validateLedger } from './native-parity-ledger.mjs'
import { readPng, verifyPng } from './parity-png.mjs'

const packages = ['hexforge-core', 'hexforge-controller', 'hexforge-native']
const sha = bytes => createHash('sha256').update(bytes).digest('hex')
function localPath(root, path) {
  if (typeof path !== 'string' || !path) throw new Error('Missing repository artifact path')
  const full = resolve(root, path), rel = relative(root, full)
  if (rel === '..' || rel.startsWith('../') || rel.startsWith('..\\') || /^[A-Za-z]:/.test(rel)) throw new Error('Artifact escapes repository')
  return full
}
export async function nativeFingerprint(root) {
  const hash = createHash('sha256')
  async function visit(path) {
    for (const entry of (await readdir(path, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
      const full = resolve(path, entry.name)
      if (entry.isDirectory()) await visit(full)
      else if (entry.isFile()) hash.update(relative(root, full).replaceAll('\\', '/')).update('\0').update(await readFile(full)).update('\0')
      else throw new Error('Native input symlinks need explicit provenance')
    }
  }
  for (const file of ['Cargo.toml', 'Cargo.lock']) hash.update(file).update(await readFile(resolve(root, file)))
  for (const name of packages) {
    const path = resolve(root, 'crates', name)
    hash.update(name).update(await readFile(resolve(path, 'Cargo.toml')))
    for (const folder of ['src', 'tests']) await visit(resolve(path, folder))
    if (name === 'hexforge-native') { await visit(resolve(path, 'assets')); await visit(resolve(path, 'examples')); hash.update(await readFile(resolve(path, 'build.rs'))) }
  }
  // Native integration tests execute this development verifier. Its behavior
  // must not change while previously recorded native evidence stays current.
  const verifier = 'scripts/verify_portable_native.ps1'
  try { const bytes = await readFile(resolve(root, verifier)); hash.update(verifier).update(bytes) }
  catch (error) { if (error.code !== 'ENOENT') throw error }
  // Local Cargo patches are shipping code too. Evidence for the app crates
  // cannot remain current after their window/render integration changes.
  const vendor = resolve(root, 'vendor')
  try { await visit(vendor) }
  catch (error) { if (error.code !== 'ENOENT' || error.path !== vendor) throw error }
  return hash.digest('hex')
}

export function parseCargoTests(output) {
  let target = null
  const tests = []
  for (const line of output.replace(/\x1b\[[0-9;]*m/g, '').split(/\r?\n/)) {
    const running = line.match(/Running (?:unittests )?([^()]+?) \(/)
    if (running) target = running[1].trim().replaceAll('\\', '/')
    const test = line.match(/^test (\S+) \.\.\. (ok|FAILED|ignored)(?:, .*)?$/)
    if (target && test) tests.push({ target, test: test[1], result: test[2] === 'ok' ? 'passed' : test[2] === 'FAILED' ? 'failed' : 'ignored' })
  }
  return tests
}
async function cargo(root, name) {
  return new Promise((accept, reject) => {
    const child = spawn('cargo', ['test', '--offline', '-p', name], { cwd: root, windowsHide: true, env: { ...process.env, CARGO_TERM_COLOR: 'never' } })
    let output = '', length = 0
    const append = bytes => { length += bytes.length; if (length > 16 * 1024 * 1024) { child.kill(); reject(new Error('Unbounded Cargo evidence output')); return }; output += bytes.toString('utf8') }
    child.stdout.on('data', append); child.stderr.on('data', append)
    child.on('error', reject)
    child.on('close', code => accept({ exitCode: code, output }))
  })
}
export async function collectNativeExecution(root, destination) {
  const path = localPath(root, destination)
  await mkdir(dirname(path), { recursive: true })
  const fingerprint = await nativeFingerprint(root), runs = []
  for (const name of packages) {
    console.log(`Executing ${name} assertions for current parity evidence…`)
    const run = await cargo(root, name)
    const artifact = relative(root, resolve(dirname(path), `${name}-execution.log`)).replaceAll('\\', '/')
    await writeFile(localPath(root, artifact), run.output)
    runs.push({ package: name, command: `cargo test --offline -p ${name}`, exitCode: run.exitCode, artifact, sha256: sha(run.output), tests: parseCargoTests(run.output) })
    if (run.exitCode !== 0) break
  }
  const current = await nativeFingerprint(root)
  const result = { fingerprint, createdAt: new Date().toISOString(), result: current === fingerprint && runs.length === packages.length && runs.every(r => r.exitCode === 0) ? 'passed' : 'failed', runs }
  await writeFile(path, JSON.stringify(result, null, 2) + '\n')
  return result
}

const referenceArchive = 'crates/hexforge-controller/tests/fixtures/reference-assertions.json'
// Freeze only original test bytes, not an executable frontend or its dependencies.
// Hash validation and create-new publication keep cutover from erasing provenance.
async function archiveReferenceAssertions(root, ledger, destination) {
  const errors = validateLedger(ledger, { requireComplete: true })
  if (errors.length) throw new Error(errors.join('\n'))
  const files = new Map()
  for (const entry of ledger.entries) {
    const bytes = await readFile(localPath(root, entry.source.path))
    if (sha(bytes) !== entry.source.hash) throw new Error(`Source assertion identity changed: ${entry.id}`)
    const previous = files.get(entry.source.path)
    if (previous && previous.sha256 !== entry.source.hash) throw new Error('Inconsistent source assertion hashes')
    files.set(entry.source.path, { path: entry.source.path, sha256: entry.source.hash, bytesBase64: bytes.toString('base64') })
  }
  const bytes = JSON.stringify({ schema: 1, purpose: 'Original assertion provenance; not runtime code', files: [...files.values()] }, null, 2) + '\n'
  if (Buffer.byteLength(bytes) > 4 * 1024 * 1024) throw new Error('Reference assertion archive exceeds its bounded size')
  const output = localPath(root, destination)
  await mkdir(dirname(output), { recursive: true })
  await writeFile(output, bytes, { flag: 'wx' })
  return files.size
}

export async function allBehavioralAssertionsHaveNativeEvidence(ledger, execution, { root = '.', requireComplete = true } = {}) {
  root = resolve(root)
  const errors = validateLedger(ledger, { requireComplete })
  if (execution?.result !== 'passed' || execution.runs?.length !== packages.length) {
    return [...errors, 'Current native execution evidence is absent or failed']
  }
  if (!isDeepStrictEqual(execution.runs.map(r => r.package).sort(), [...packages].sort()))
    errors.push('Current native execution package set differs; every owning crate must execute exactly once')
  if (execution.fingerprint !== await nativeFingerprint(root)) errors.push('Current native execution fingerprint differs; rerun after code/test changes')
  for (const run of execution.runs) {
    if (run.command !== `cargo test --offline -p ${run.package}`) errors.push(`Current native execution command differs: ${run.package}`)
    try {
      const output = await readFile(localPath(root, run.artifact))
      if (sha(output) !== run.sha256 || run.exitCode !== 0 || !packages.includes(run.package)
        || JSON.stringify(parseCargoTests(output.toString('utf8'))) !== JSON.stringify(run.tests)) errors.push(`Current native execution artifact differs: ${run.package}`)
    } catch { errors.push(`Missing current native execution artifact: ${run.package}`) }
  }
  let archived
  async function sourceBytes(source) {
    try { return await readFile(localPath(root, source.path)) }
    catch (error) { if (error.code !== 'ENOENT') throw error }
    if (!archived) {
      const path = localPath(root, referenceArchive)
      if ((await stat(path)).size > 4 * 1024 * 1024) throw new Error('Unbounded reference assertion archive')
      const archive = JSON.parse(await readFile(path, 'utf8'))
      if (archive.schema !== 1 || !Array.isArray(archive.files)) throw new Error('Invalid reference assertion archive')
      archived = new Map()
      for (const file of archive.files) {
        if (typeof file.path !== 'string' || typeof file.bytesBase64 !== 'string' || archived.has(file.path)) throw new Error('Ambiguous reference assertion archive')
        const bytes = Buffer.from(file.bytesBase64, 'base64')
        if (sha(bytes) !== file.sha256) throw new Error('Source assertion archive bytes changed')
        archived.set(file.path, bytes)
      }
    }
    const bytes = archived.get(source.path)
    if (!bytes) throw new Error('Source assertion absent from archive')
    return bytes
  }
  for (const entry of ledger.entries ?? []) {
    if (entry.status !== 'passed') continue
    try { if (sha(await sourceBytes(entry.source)) !== entry.source.hash) errors.push(`Source assertion identity changed: ${entry.id}`) }
    catch { errors.push(`Source assertion file/archive missing or invalid: ${entry.id}`) }
    const assertions = entry.native?.assertions ?? [{ file: entry.native?.file, test: entry.native?.test }]
    if (!assertions.length || !entry.native?.rationale) errors.push(`Missing assertion-specific mapping rationale: ${entry.id}`)
    for (const assertion of assertions) {
      try { await readFile(localPath(root, assertion.file)) } catch { errors.push(`Native assertion file missing: ${entry.id}`); continue }
      const match = assertion.file?.replaceAll('\\', '/').match(/^crates\/([^/]+)\/(.+)$/)
      const target = assertion.target ?? match?.[2]
      const run = execution.runs.find(r => r.package === match?.[1])
      if (!run?.tests.some(t => t.target === target && t.test === assertion.test && t.result === 'passed')) errors.push(`Native assertion did not execute successfully: ${entry.id} → ${assertion.file}#${assertion.test}`)
    }
  }
  return errors
}

export const requiredVisualScenes = ['empty', 'open', 'open-32', 'empty-file', 'edited', 'search', 'goto', 'file-menu', 'edit-menu', 'navigate-menu', 'template-menu', 'view-menu', 'minimap-menu', 'results-menu', 'busy', 'error', 'pending-template', 'fit-top', 'fit-middle', 'fit-eof', 'proportional-top', 'proportional-middle', 'proportional-eof', 'selected-field', 'cleared-field', 'results-diagnostics', 'results-tree-focus', 'editor-empty', 'editor-advanced', 'editor-invalid', 'editor-dirty', 'editor-close', 'editor-save-error', 'binary-close']

/** Raw, unmasked lossless pixel measurements. No scaling, thresholds, or
 * automatic "passed" classification: source/native review remains mandatory. */
export function compareCapturePixels(referenceBytes, nativeBytes) {
  const a = readPng(referenceBytes), b = readPng(nativeBytes, a.width, a.height)
  let changedPixels = 0, maximumChannelDelta = 0, left = a.width, top = a.height, right = -1, bottom = -1
  for (let offset = 0; offset < a.rgba.length; offset += 4) {
    let changed = false
    for (let channel = 0; channel < 4; channel++) {
      const delta = Math.abs(a.rgba[offset + channel] - b.rgba[offset + channel])
      maximumChannelDelta = Math.max(maximumChannelDelta, delta)
      changed ||= delta !== 0
    }
    if (changed) {
      changedPixels++
      const pixel = offset / 4, x = pixel % a.width, y = Math.floor(pixel / a.width)
      left = Math.min(left, x); top = Math.min(top, y); right = Math.max(right, x); bottom = Math.max(bottom, y)
    }
  }
  return { physicalSize: [a.width, a.height], changedPixels, maximumChannelDelta,
    differenceBounds: changedPixels ? [left, top, right - left + 1, bottom - top + 1] : null }
}
export async function allRequiredVisualStatesHaveMatchedCaptures(manifest, { root = '.', scenes = requiredVisualScenes } = {}) {
  const errors = []
  for (const scene of scenes) for (const theme of ['dark', 'light']) {
    const key = `${scene}/${theme}`
    const pairs = (manifest?.captures ?? []).filter(c => c.scene === scene && c.theme === theme)
    if (pairs.length !== 1) { errors.push(`${pairs.length ? 'Duplicate' : 'Missing'} visual pair: ${key}`); continue }
    const pair = pairs[0]
    if (!pair.verified || pair.comparison?.result !== 'passed') { errors.push(`Unverified visual pair: ${key}`); continue }
    const observed = {}
    for (const side of ['reference', 'native']) {
      try {
        const capture = pair[side], metadataBytes = await readFile(localPath(root, capture.metadata))
        const metadata = JSON.parse(metadataBytes.toString('utf8'))
        const png = await readFile(localPath(root, capture.file))
        if (sha(png) !== capture.sha256 || metadata.evidence !== 'lossless' || metadata.scene !== scene || metadata.theme !== theme
          || !Number.isFinite(metadata.dpr) || metadata.dpr <= 0 || metadata.clientSize?.length !== 2
          || !metadata.clientSize.every(n => Number.isSafeInteger(n) && n > 0)
          || !Object.hasOwn(metadata, 'focusLabel') || !metadata.state || typeof metadata.state !== 'object')
          throw new Error('Missing actual matching capture provenance')
        verifyPng(png, Math.round(metadata.clientSize[0] * metadata.dpr), Math.round(metadata.clientSize[1] * metadata.dpr))
        observed[side] = { metadata, metadataSha256: sha(metadataBytes), png }
      } catch { errors.push(`Missing or mismatched lossless capture: ${key}/${side}`) }
    }
    if (observed.reference && observed.native) {
      const a = observed.reference.metadata, b = observed.native.metadata
      if (!isDeepStrictEqual(a.clientSize, b.clientSize)) errors.push(`Client sizes differ: ${key}`)
      if (a.dpr !== b.dpr) errors.push(`DPI differs: ${key}`)
      if (a.focusLabel !== b.focusLabel) errors.push(`Focus differs: ${key}`)
      if (!isDeepStrictEqual(a.state, b.state)) errors.push(`Application state differs: ${key}`)
    }
    if (!pair.comparison?.artifact || !pair.comparison?.reviewedDifferences) errors.push(`Missing actual comparison review: ${key}`)
    try {
      const artifact = await readFile(localPath(root, pair.comparison.artifact)), comparison = JSON.parse(artifact.toString('utf8'))
      if (sha(artifact) !== pair.comparison.sha256 || comparison.schema !== 1 || comparison.scene !== scene
          || comparison.theme !== theme || comparison.result !== 'passed'
          || comparison.reviewedDifferences !== pair.comparison.reviewedDifferences)
        throw new Error('Comparison review/provenance changed')
      for (const side of ['reference', 'native']) {
        if (!observed[side] || comparison[side]?.sha256 !== pair[side].sha256
            || comparison[side]?.metadataSha256 !== observed[side].metadataSha256)
          throw new Error('Comparison refers to a different image or snapshot')
      }
      if (!isDeepStrictEqual(comparison.pixelSummary, compareCapturePixels(observed.reference.png, observed.native.png)))
        errors.push(`Actual comparison pixel summary differs: ${key}`)
    } catch { errors.push(`Missing or mismatched comparison artifact: ${key}`) }
  }
  return errors
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [mode, path, evidence] = process.argv.slice(2)
  if (mode === '--collect-native' && path) {
    const result = await collectNativeExecution(resolve('.'), path)
    console.log(`Current native execution: ${result.result}; ${result.runs.reduce((sum, run) => sum + run.tests.filter(t => t.result === 'passed').length, 0)} assertions`)
    if (result.result !== 'passed') process.exitCode = 1
  } else if (mode === '--archive-reference' && path && evidence) {
    console.log(`Archived ${await archiveReferenceAssertions(resolve('.'), JSON.parse(await readFile(path, 'utf8')), evidence)} original assertion files without changing their bytes`)
  } else if (mode === '--check-ledger' && path && evidence) {
    const errors = await allBehavioralAssertionsHaveNativeEvidence(JSON.parse(await readFile(path, 'utf8')), JSON.parse(await readFile(evidence, 'utf8')))
    console.log(errors.length ? errors.join('\n') : 'All source assertions have current native evidence')
    if (errors.length) process.exitCode = 1
  } else if (mode === '--check-visual' && path) {
    const errors = await allRequiredVisualStatesHaveMatchedCaptures(JSON.parse(await readFile(path, 'utf8')))
    console.log(errors.length ? errors.join('\n') : 'All required visual states have matched captures')
    if (errors.length) process.exitCode = 1
  } else throw new Error('Use --collect-native OUTPUT.json, --archive-reference LEDGER.json ARCHIVE.json, --check-ledger LEDGER.json EXECUTION.json, or --check-visual MANIFEST.json')
}
