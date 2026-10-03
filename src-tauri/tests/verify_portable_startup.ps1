param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath,
    [int]$TimeoutSeconds = 20,
    [switch]$Measure,
    [string]$ProfileOutputPath,
    [string]$WebViewProfilePath
)

$ErrorActionPreference = 'Stop'
if ($TimeoutSeconds -lt 1) { throw 'TimeoutSeconds must be positive.' }
$exe = (Resolve-Path -LiteralPath $ExecutablePath).Path
$probeName = 'HEXFORGE_PORTABLE_SMOKE_TEST'
$readyTitle = 'HexForge - Ready'
$previous = [Environment]::GetEnvironmentVariable($probeName, 'Process')
$profileName = 'HEXFORGE_STARTUP_PROFILE'
$previousProfile = [Environment]::GetEnvironmentVariable($profileName, 'Process')
$dataDirName = 'HEXFORGE_STARTUP_PROFILE_DATA_DIR'
$previousDataDir = [Environment]::GetEnvironmentVariable($dataDirName, 'Process')
$profilePath = $null
if ($Measure -and -not $ProfileOutputPath) {
    $temporaryReport = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ('hexforge-paint-' + [guid]::NewGuid().ToString('N')))
    $ProfileOutputPath = Join-Path $temporaryReport.FullName 'timings.json'
}
if ($ProfileOutputPath) {
    $profilePath = [System.IO.Path]::GetFullPath($ProfileOutputPath)
    if (-not (Test-Path -LiteralPath ([System.IO.Path]::GetDirectoryName($profilePath)) -PathType Container)) {
        throw 'The timing report output directory must already exist.'
    }
}
$profileDirectory = $null
if ($WebViewProfilePath) {
    if (-not $profilePath) { throw 'An isolated WebView profile requires opt-in profiling.' }
    $profileDirectory = [System.IO.Path]::GetFullPath($WebViewProfilePath)
    if (-not (Test-Path -LiteralPath $profileDirectory -PathType Container)) { $null = New-Item -ItemType Directory -Path $profileDirectory }
}
$process = $null
$nativeWindowMs = $null
$clock = [System.Diagnostics.Stopwatch]::new()

try {
    # Only this child inherits the flag. In ordinary runs the title is unchanged.
    [Environment]::SetEnvironmentVariable($probeName, '1', 'Process')
    [Environment]::SetEnvironmentVariable($profileName, $profilePath, 'Process')
    [Environment]::SetEnvironmentVariable($dataDirName, $profileDirectory, 'Process')
    $clock.Start()
    # Real-paint measurements require a visible interface. A normal smoke run
    # remains hidden and verifies bundled frontend/IPC readiness, not paint.
    if ($profilePath) { $process = Start-Process -FilePath $exe -PassThru -WindowStyle Normal }
    else { $process = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden }
} finally {
    [Environment]::SetEnvironmentVariable($probeName, $previous, 'Process')
    [Environment]::SetEnvironmentVariable($profileName, $previousProfile, 'Process')
    [Environment]::SetEnvironmentVariable($dataDirName, $previousDataDir, 'Process')
}

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $process.Refresh()
        if ($process.HasExited) {
            throw "HexForge exited before its frontend became ready (exit code $($process.ExitCode))."
        }
        if ($null -eq $nativeWindowMs -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
            $nativeWindowMs = $clock.Elapsed.TotalMilliseconds
        }
        if ($process.MainWindowTitle -eq $readyTitle) {
            if ($Measure) {
                if ((Get-Item -LiteralPath $profilePath).LastWriteTimeUtc -lt $process.StartTime.ToUniversalTime()) {
                    throw 'The timing report predates this launch; refusing a stale paint measurement.'
                }
                $report = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json
                if ($report.measurementError) { throw "Paint measurement failed: $($report.measurementError)" }
                if (-not $report.metrics -or $report.metrics.clockOrigin -ne 'windows-process-creation') {
                    throw 'No verified process-creation-to-main-interface paint metric. Mount/title readiness is not paint.'
                }
                [PSCustomObject]@{
                    ExecutablePath = $exe
                    NativeWindowMs = [Math]::Round($nativeWindowMs, 1)
                    FrontendReadyMs = [Math]::Round($clock.Elapsed.TotalMilliseconds, 1)
                    FirstMeaningfulPaintMs = [Math]::Round($report.metrics.firstMeaningfulPaintMs, 3)
                    InteractiveMs = [Math]::Round($report.metrics.interactiveMs, 3)
                    ClockRoundTripUncertaintyMs = [Math]::Round($report.metrics.clockRoundTripUncertaintyMs, 3)
                    ReportPath = $profilePath
                }
            } else {
                Write-Output "Verified portable frontend startup: $exe"
            }
            return
        }
        if ($Measure) { Start-Sleep -Milliseconds 10 }
        else { Start-Sleep -Milliseconds 200 }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "HexForge did not signal frontend readiness within $TimeoutSeconds seconds. Last title: '$($process.MainWindowTitle)'."
} finally {
    if ($process -and -not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(3000)) {
            # This process was created by the smoke test; leave no hidden GUI behind.
            Stop-Process -Id $process.Id -Force
        }
    }
}
