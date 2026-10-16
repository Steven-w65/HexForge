param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'This integration check must run on Windows.'
}
$nativeRepository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $nativeRepository
try {
    & cargo test --offline --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Native workspace regressions failed.' }
    & cargo test --offline -p hexforge-native --example import_results_preferences
    if ($LASTEXITCODE -ne 0) { throw 'Preference importer regressions failed.' }
    & cargo fmt --all --check
    if ($LASTEXITCODE -ne 0) { throw 'Native formatting check failed.' }
    & cargo check --offline --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Native workspace check failed.' }
    if (-not $SkipBuild) {
        & cargo build --offline -p hexforge-native
        if ($LASTEXITCODE -ne 0) { throw 'Native development build failed. Close only this build if Windows holds its executable open.' }
    }
    Write-Output 'Automated native Windows/controller/widget checks passed.'
    Write-Output 'Real dialog/focus/IME/OS-theme checks and paired screenshots require separately recorded observations; this script does not claim they passed.'
} finally {
    Pop-Location
}
