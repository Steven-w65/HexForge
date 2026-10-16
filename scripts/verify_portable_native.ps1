<#
Copy-only preflight. Requires PowerShell7; does not launch, close, or automate
Windows applications, change policy, install dependencies, or delete anything.
Live workflow/startup verification remains a separate, mandatory evidence gate.
#>
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Destination,
    [Parameter(Mandatory)][string]$Report,
    [switch]$PrepareOnly
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Portable Windows preflight must run on Windows.'
}
if (-not $PrepareOnly) {
    throw 'Use -PrepareOnly. This script cannot claim live workflow or startup verification.'
}
foreach ($path in @($Executable, $Destination, $Report)) {
    if (-not [IO.Path]::IsPathFullyQualified($path)) {
        throw 'Use explicit absolute executable, new destination, and report paths.'
    }
}
$source = (Resolve-Path -LiteralPath $Executable).ProviderPath
$portableDirectory = [IO.Path]::GetFullPath($Destination).TrimEnd([IO.Path]::DirectorySeparatorChar)
$reportPath = [IO.Path]::GetFullPath($Report)
if ([IO.Path]::GetExtension($source) -ne '.exe' -or -not [IO.File]::Exists($source)) {
    throw 'The source must be an existing Windows executable.'
}
if (Test-Path -LiteralPath $portableDirectory) {
    throw 'The portable destination already exists; nothing will be replaced.'
}
if (Test-Path -LiteralPath $reportPath) {
    throw 'The report already exists; earlier evidence will not be overwritten.'
}
if ([IO.Path]::GetExtension($reportPath) -ne '.json' -or
    $reportPath.Equals($portableDirectory, [StringComparison]::OrdinalIgnoreCase) -or
    $reportPath.StartsWith($portableDirectory + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a new JSON report outside the portable executable directory.'
}
foreach ($parent in @([IO.Path]::GetDirectoryName($portableDirectory),
    [IO.Path]::GetDirectoryName($reportPath))) {
    if (-not [IO.Directory]::Exists($parent)) { throw 'Destination and report parents must already exist.' }
}
$length = (Get-Item -LiteralPath $source).Length
if ($length -lt 64 -or $length -gt 128MB) { throw 'Source executable size is invalid or exceeds the bounded preflight read.' }
$bytes = [IO.File]::ReadAllBytes($source)
if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Source has no Windows MZ executable header.' }
$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
if ($peOffset -lt 64 -or $peOffset -gt $bytes.Length - 26 -or
    [BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550) {
    throw 'Source has an invalid or truncated Windows PE header.'
}
$machine = [BitConverter]::ToUInt16($bytes, $peOffset + 4)
$optionalSize = [BitConverter]::ToUInt16($bytes, $peOffset + 20)
if ($machine -notin @(0x8664, 0xaa64) -or $optionalSize -lt 112 -or
    $optionalSize -gt $bytes.Length - $peOffset - 24 -or
    [BitConverter]::ToUInt16($bytes, $peOffset + 24) -ne 0x20b) {
    throw 'Source is not a supported 64-bit Windows PE executable.'
}
$sourceHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes))
# New-Item has no -Force; FileMode.CreateNew also refuses races/overwrites.
[void](New-Item -ItemType Directory -Path $portableDirectory)
$portableExe = Join-Path $portableDirectory 'hexforge.exe'
$copy = [IO.File]::Open($portableExe, [IO.FileMode]::CreateNew,
    [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $copy.Write($bytes, 0, $bytes.Length); $copy.Flush($true) }
finally { $copy.Dispose() }
$portableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $portableExe).Hash
if ($portableHash -ne $sourceHash -or
    (Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash -ne $sourceHash) {
    throw 'Source changed during preparation, or portable copy differs. No passing report will be written.'
}
$files = @(Get-ChildItem -LiteralPath $portableDirectory -Force)
if ($files.Count -ne 1 -or $files[0].Name -ne 'hexforge.exe' -or $files[0].PSIsContainer) {
    throw 'Portable directory must contain only the exact executable.'
}
$evidence = [ordered]@{
    schema = 1
    status = 'preflight-passed; live-verification-pending'
    executable = $source
    portableExecutable = $portableExe
    sourceSha256 = $sourceHash
    portableSha256 = $portableHash
    bytes = $bytes.Length
    files = @('hexforge.exe')
    liveVerified = $false
    startupMeasured = $false
    note = 'No application was launched. Real controls, workflows, process/dependency behavior, window cleanup and presented-interface startup must be verified separately on this exact hash.'
}
$output = [Text.Encoding]::UTF8.GetBytes(($evidence | ConvertTo-Json -Depth 4) + [Environment]::NewLine)
$reportFile = [IO.File]::Open($reportPath, [IO.FileMode]::CreateNew,
    [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $reportFile.Write($output, 0, $output.Length); $reportFile.Flush($true) }
finally { $reportFile.Dispose() }
Write-Output "Portable preflight passed: $portableExe; live verification remains pending."
