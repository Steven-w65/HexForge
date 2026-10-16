param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath
)

$ErrorActionPreference = "Stop"

$resolvedPath = (Resolve-Path -LiteralPath $ExecutablePath).Path
$bytes = [System.IO.File]::ReadAllBytes($resolvedPath)

if ($bytes.Length -lt 0x40 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
    throw "Not a valid Windows executable: $resolvedPath"
}

$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
$subsystemOffset = $peOffset + 24 + 68

if ($subsystemOffset + 2 -gt $bytes.Length) {
    throw "The Windows executable header is incomplete: $resolvedPath"
}

$subsystem = [BitConverter]::ToUInt16($bytes, $subsystemOffset)
$windowsGuiSubsystem = 2

if ($subsystem -ne $windowsGuiSubsystem) {
    throw "Expected Windows GUI subsystem 2, but found subsystem $subsystem in $resolvedPath"
}

Write-Output "Verified Windows GUI subsystem: $resolvedPath"
