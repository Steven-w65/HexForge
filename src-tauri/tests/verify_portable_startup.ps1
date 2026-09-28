param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath,
    [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = 'Stop'
if ($TimeoutSeconds -lt 1) { throw 'TimeoutSeconds must be positive.' }
$exe = (Resolve-Path -LiteralPath $ExecutablePath).Path
$probeName = 'HEXFORGE_PORTABLE_SMOKE_TEST'
$readyTitle = 'HexForge - Ready'
$previous = [Environment]::GetEnvironmentVariable($probeName, 'Process')
$process = $null

try {
    # Only this child inherits the flag. In ordinary runs the title is unchanged.
    [Environment]::SetEnvironmentVariable($probeName, '1', 'Process')
    $process = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
} finally {
    [Environment]::SetEnvironmentVariable($probeName, $previous, 'Process')
}

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $process.Refresh()
        if ($process.HasExited) {
            throw "HexForge exited before its frontend became ready (exit code $($process.ExitCode))."
        }
        if ($process.MainWindowTitle -eq $readyTitle) {
            Write-Output "Verified portable frontend startup: $exe"
            exit 0
        }
        Start-Sleep -Milliseconds 200
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
