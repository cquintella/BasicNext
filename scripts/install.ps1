<#
.SYNOPSIS
    Basic Next installer for Windows.

.DESCRIPTION
    Installs the bni.exe (interpreter), bnc.exe (compiler) and bn.exe (dispatcher)
    binaries plus the runtime files they discover
    (stdlib .bn modules, diagnostics catalog, and native runtime lib) under a prefix:

        <Prefix>\bin\bni.exe
        <Prefix>\bin\bnc.exe
        <Prefix>\bin\bn.exe
        <Prefix>\lib\bn_rt.lib                    native runtime for `bnc`
        <Prefix>\share\bn\modules\bn\*.bn         standard library modules
        <Prefix>\share\bn\diagnostics\en-US\*.ftl diagnostics catalog (optional;
                                                  an identical catalog is embedded)

    bni finds modules\bn and the catalog by walking upward from its own location,
    and finds bn_rt.lib next to the binary, in <Prefix>\, or in <Prefix>\lib\
    (override with BN_RT_LIB). Do not point BN_RT_LIB at a source-tree
    target\ directory for a normal install. The prefix's bin directory is
    added to the current user's PATH.

.PARAMETER Prefix
    Install root. Default: $env:LOCALAPPDATA\Programs\BasicNext (no admin needed).

.PARAMETER NoBuild
    Install already-built target\release binaries instead of rebuilding.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prefix C:\Tools\BasicNext
#>
param(
    [string]$Prefix = "$env:LOCALAPPDATA\Programs\BasicNext",
    [switch]$NoBuild
)
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $repoRoot

if (-not $NoBuild) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo (Rust 1.97+) is required to build; pass -NoBuild to install prebuilt binaries."
    }
    Write-Host "==> Building release binaries (cargo build --release --workspace --bins)"
    cargo build --release --workspace --bins
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    Write-Host "==> Building native runtime (cargo build -p bn_rt --release)"
    cargo build -p bn_rt --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p bn_rt failed" }
}

$bniBin = Join-Path $repoRoot 'target\release\bni.exe'
$bncBin = Join-Path $repoRoot 'target\release\bnc.exe'
$bnBin  = Join-Path $repoRoot 'target\release\bn.exe'
$bnRtLib = Join-Path $repoRoot 'target\release\bn_rt.lib'
foreach ($bin in @($bniBin, $bncBin, $bnBin)) {
    if (-not (Test-Path $bin)) { throw "missing binary '$bin' (build first, or drop -NoBuild)" }
}
if (-not (Test-Path $bnRtLib)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "==> bn_rt.lib missing; building cargo -p bn_rt --release"
        cargo build -p bn_rt --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build -p bn_rt failed" }
    }
}
if (-not (Test-Path $bnRtLib)) {
    throw "missing '$bnRtLib' (needed for bnc). Build with cargo -p bn_rt --release."
}

$binDir  = Join-Path $Prefix 'bin'
$libDir  = Join-Path $Prefix 'lib'
$modDir  = Join-Path $Prefix 'share\bn\modules\bn'
$diagDir = Join-Path $Prefix 'share\bn\diagnostics'

Write-Host "==> Installing to $Prefix"
foreach ($dir in @($binDir, $libDir, $modDir, $diagDir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}
Copy-Item $bniBin (Join-Path $binDir 'bni.exe') -Force
Copy-Item $bncBin (Join-Path $binDir 'bnc.exe') -Force
Copy-Item $bnBin  (Join-Path $binDir 'bn.exe')  -Force
Copy-Item $bnRtLib (Join-Path $libDir 'bn_rt.lib') -Force
Copy-Item (Join-Path $repoRoot 'modules\bn\*.bn') $modDir -Force
Copy-Item (Join-Path $repoRoot 'share\bn\diagnostics\*') $diagDir -Recurse -Force

Write-Host "==> Installed:"
Write-Host "    $binDir\bni.exe, $binDir\bnc.exe, $binDir\bn.exe"
Write-Host "    $libDir\bn_rt.lib"
Write-Host "    $modDir\, $diagDir\"

# Add the bin directory to the user's PATH (idempotent).
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $binDir) {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $binDir } else { "$userPath;$binDir" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "==> Added $binDir to your user PATH (restart the shell to pick it up)."
}

# Verify against the installed copy, from a clean working directory.
Write-Host "==> Verifying"
& (Join-Path $binDir 'bni.exe') --version
& (Join-Path $binDir 'bnc.exe') --version
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("bn-check-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$check = Join-Path $tmp 'check.bn'
"IMPORT BNMath AS M`nFUNCTION Start() AS VOID`nPRINT M.ABS(-7.0)`nEND FUNCTION" | Set-Content -NoNewline -Path $check
Push-Location $tmp
try {
    $out = (& (Join-Path $binDir 'bni.exe') run check.bn) 2>&1
    if ("$out".Trim() -eq '7.0') {
        Write-Host "    stdlib module resolution OK (BNMath.ABS(-7.0) = 7.0)"
    } else {
        Write-Warning "stdlib check did not return the expected value; output was: $out"
    }
} finally {
    Pop-Location
    Remove-Item -Recurse -Force $tmp
}
Write-Host "==> Done."
