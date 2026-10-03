import { spawn } from 'node:child_process'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** No trimming: the nearest-rank 95th percentile includes slow valid launches. */
export function summarizeLaunches(reports) {
  if (!reports.length) throw new RangeError('At least one verified launch is required.')
  for (const report of reports) {
    const metric = report.metrics
    if (metric?.clockOrigin !== 'windows-process-creation' || report.measurementError ||
        !Number.isFinite(metric.firstMeaningfulPaintMs) || metric.firstMeaningfulPaintMs <= 0 ||
        !Number.isFinite(metric.interactiveMs) || metric.interactiveMs < metric.firstMeaningfulPaintMs) {
      throw new RangeError('A launch has no verified process-to-meaningful-paint measurement.')
    }
  }
  const stats = key => {
    const values = reports.map(report => report.metrics[key]).sort((a, b) => a - b)
    const middle = Math.floor(values.length / 2)
    return {
      medianMs: values.length % 2 ? values[middle] : (values[middle - 1] + values[middle]) / 2,
      p95Ms: values[Math.ceil(values.length * .95) - 1], fastestMs: values[0],
    }
  }
  return { count: reports.length, firstMeaningfulPaint: stats('firstMeaningfulPaintMs'), interactive: stats('interactiveMs') }
}

export function variantOrder(variants, iteration) {
  return iteration % 2 ? [...variants].reverse() : [...variants]
}

function runProbe(executable, reportPath, profilePath) {
  return new Promise((accept, reject) => {
    const script = resolve('src-tauri/tests/verify_portable_startup.ps1')
    const child = spawn('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script,
      '-ExecutablePath', executable, '-Measure', '-ProfileOutputPath', reportPath, '-WebViewProfilePath', profilePath],
      { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] })
    let output = ''
    child.stdout.on('data', chunk => { output += chunk })
    child.stderr.on('data', chunk => { output += chunk })
    child.on('error', reject)
    child.on('exit', code => code === 0 ? accept() : reject(new Error(`Startup probe failed (${code}): ${output}`)))
  })
}

async function main() {
  if (process.platform !== 'win32') throw new Error('The native benchmark requires Windows.')
  const args = new Map()
  for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i], process.argv[i + 1])
  const runs = Number(args.get('--runs') ?? 20)
  if (!Number.isSafeInteger(runs) || runs < 2 || runs > 100) throw new RangeError('Use 2–100 measured launches per category.')
  const executable = resolve(args.get('--exe') ?? 'src-tauri/target/release/hexforge.exe')
  const output = resolve(args.get('--output-dir') ?? 'src-tauri/target/startup-benchmark')
  const label = args.get('--label') ?? 'candidate'
  if (!/^[a-z0-9-]+$/.test(label)) throw new Error('Use a simple lowercase benchmark label.')
  await mkdir(output, { recursive: true })
  const warmProfile = resolve(output, 'warm-profile')
  const variants = args.has('--baseline-exe')
    ? { baseline: resolve(args.get('--baseline-exe')), candidate: executable } : { [label]: executable }
  const reports = Object.fromEntries(Object.keys(variants).map(variant => [variant, { warm: [], 'profile-cold': [] }]))
  const kinds = (args.get('--kinds') ?? 'warm,profile-cold').split(',')
  if (kinds.some(kind => !['warm', 'profile-cold'].includes(kind))) throw new Error('Kinds must be warm and/or profile-cold.')
  // One explicit warmup per binary is excluded by definition, not after seeing its result.
  for (const [variant, exe] of Object.entries(variants)) {
    await runProbe(exe, resolve(output, `${label}-${variant}-warmup.json`), warmProfile)
  }
  for (let i = 0; i < runs; i++) {
    // Alternate categories to reduce time-of-run bias. Profile-cold means a
    // fresh WebView data directory, NOT flushed OS caches or a reboot.
    for (const kind of kinds) {
      for (const variant of variantOrder(Object.keys(variants), i)) {
        const path = resolve(output, `${label}-${variant}-${kind}-${i + 1}.json`)
        const profile = kind === 'warm' ? warmProfile : resolve(output, `${label}-${variant}-fresh-profile-${i + 1}`)
        // Never label an existing profile as cold when a command is repeated.
        if (kind === 'profile-cold') await mkdir(profile)
        await runProbe(variants[variant], path, profile)
        const report = JSON.parse(await readFile(path, 'utf8'))
        summarizeLaunches([report]) // Reject invalid runs; never turn failures into zero ms.
        reports[variant][kind].push(report)
        console.log(`${variant} ${kind} ${i + 1}/${runs}: paint=${report.metrics.firstMeaningfulPaintMs.toFixed(1)} ms, interactive=${report.metrics.interactiveMs.toFixed(1)} ms`)
      }
    }
  }
  const summary = { label, executable, method: 'visible browser contentful paint gated on complete main controls; native process-creation clock, minimum-RTT correlation',
    coldDefinition: 'fresh WebView profile, warm OS cache; not reboot-cold',
    statistics: 'median and nearest-rank p95; no outlier removal; one predefined warmup',
    variants, results: Object.fromEntries(Object.keys(variants).map(variant => [variant,
      Object.fromEntries(kinds.map(kind => [kind, summarizeLaunches(reports[variant][kind])]))])),
  }
  await writeFile(resolve(output, `${label}-summary.json`), `${JSON.stringify(summary, null, 2)}\n`)
  console.log(JSON.stringify(summary, null, 2))
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1 })
}
