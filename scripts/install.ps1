<#
.SYNOPSIS
    Basic Next installer for Windows.

.DESCRIPTION
    Installs the bni.exe (interpreter) and bnc.exe (compiler) binaries plus the
    runtime files they discover
    (stdlib .bn modules, diagnostics catalog, and native runtime lib) under a prefix:

        <Prefix>\bin\bni.exe
        <Prefix>\bin\bnc.exe
        <Prefix>\lib\bn_rt.lib                    native runtime for `bnc`
        <Prefix>\share\bn\modules\bn\*.bn         standard library modules
        <Prefix>\share\bn\diagnostics\en-US\*.ftl diagnostics catalog (optional;
                                                  an identical catalog is embedded)

    bni finds modules\bn and the catalog by walking upward from its own location,
    and finds bn_rt.lib next to the binary, in <Prefix>\, or in <Prefix>\lib\
    (override with BN_RT_LIB). Do not point BN_RT_LIB at a source-tree
    target\ directory for a normal install. The prefix's bin directory is
    added to the current user's PATH.

    After a successful install the script also writes, under the invoking
    user's profile (override with BN_STATE_DIR):

        %USERPROFILE%\.basicnext\install.log      append-only log of operations
        %USERPROFILE%\.basicnext\uninstall.ps1    removes the files this run installed

.PARAMETER Prefix
    Install root. Default: $env:LOCALAPPDATA\Programs\BasicNext (no admin needed).

.PARAMETER NoBuild
    Install already-built target\release binaries instead of rebuilding.

.PARAMETER NoPathUpdate
    Do not add the installed bin directory to the current user's PATH.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prefix C:\Tools\BasicNext
#>
param(
    [string]$Prefix = "$env:LOCALAPPDATA\Programs\BasicNext",
    [switch]$NoBuild,
    [switch]$NoPathUpdate
)
$ErrorActionPreference = 'Stop'

$BnStateDir = if ($env:BN_STATE_DIR) { $env:BN_STATE_DIR } else { Join-Path $env:USERPROFILE '.basicnext' }
$InstallLog = Join-Path $BnStateDir 'install.log'
$UninstallScript = Join-Path $BnStateDir 'uninstall.ps1'
$script:Manifest = New-Object System.Collections.Generic.List[string]
New-Item -ItemType Directory -Force -Path $BnStateDir | Out-Null
@(
    "===== Basic Next install $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz') ====="
    "user=$env:USERNAME host=$env:COMPUTERNAME"
    "Prefix=$Prefix BnStateDir=$BnStateDir NoBuild=$NoBuild"
) | Add-Content -Path $InstallLog

function Write-Log([string]$Message) {
    Write-Host $Message
    Add-Content -Path $InstallLog -Value $Message
}

function Record-Installed([string]$Path) {
    [void]$script:Manifest.Add($Path)
    Add-Content -Path $InstallLog -Value "    installed: $Path"
}

function Assert-ReleaseAsset(
    [string]$ChecksumsPath,
    [string]$AssetPath,
    [string]$AssetName = ''
) {
    if ([string]::IsNullOrEmpty($AssetName)) {
        $AssetName = Split-Path $AssetPath -Leaf
    }
    $pattern = '\s' + [regex]::Escape($AssetName) + '$'
    $checksumLine = Get-Content $ChecksumsPath | Where-Object { $_ -match $pattern } | Select-Object -First 1
    if (-not $checksumLine) { throw "$AssetName is absent from SHA256SUMS" }
    $expected = ($checksumLine -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "SHA256 mismatch for $AssetName (expected $expected, got $actual)"
    }
}

function Receive-ReleaseAsset(
    [string]$Base,
    [string]$AssetName,
    [string]$Destination
) {
    if (Test-Path -LiteralPath $Base -PathType Container) {
        Copy-Item (Join-Path $Base $AssetName) $Destination
        return
    }

    $Uri = "$($Base.TrimEnd('/'))/$AssetName"
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
}

$InitialLocation = Get-Location
$BootstrapDir = $null
try {
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not (Test-Path (Join-Path $repoRoot 'Cargo.toml'))) {
    $Tag = $env:BN_VERSION
    if ([string]::IsNullOrEmpty($Tag)) {
        $Latest = Invoke-RestMethod -UseBasicParsing `
            -Uri 'https://api.github.com/repos/cquintella/BasicNext/releases/latest'
        $Tag = [string]$Latest.tag_name
        if ([string]::IsNullOrEmpty($Tag)) {
            throw 'could not resolve the latest Basic Next release'
        }
    }
    if ($env:PROCESSOR_ARCHITECTURE -notin @('AMD64', 'x86_64')) {
        throw "no verified prebuilt Windows asset for architecture '$env:PROCESSOR_ARCHITECTURE'"
    }

    $BootstrapDir = Join-Path ([System.IO.Path]::GetTempPath()) ("basicnext-install-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $BootstrapDir | Out-Null
    $Base = if ($env:BN_RELEASE_BASE) {
        $env:BN_RELEASE_BASE
    } else {
        "https://github.com/cquintella/BasicNext/releases/download/$Tag"
    }
    $Checksums = Join-Path $BootstrapDir 'SHA256SUMS'
    Receive-ReleaseAsset $Base 'SHA256SUMS' $Checksums

    $SourceName = "basicnext-source-$Tag.zip"
    $SourceArchive = Join-Path $BootstrapDir $SourceName
    Receive-ReleaseAsset $Base $SourceName $SourceArchive
    Assert-ReleaseAsset $Checksums $SourceArchive $SourceName

    foreach ($AssetName in @(
            'bni-windows-x86_64.exe',
            'bnc-windows-x86_64.exe',
            'bn_rt-windows-x86_64.lib'
        )) {
        $AssetPath = Join-Path $BootstrapDir $AssetName
        Receive-ReleaseAsset $Base $AssetName $AssetPath
        Assert-ReleaseAsset $Checksums $AssetPath $AssetName
    }

    Expand-Archive -LiteralPath $SourceArchive -DestinationPath $BootstrapDir -Force
    $SourceRoot = Get-ChildItem $BootstrapDir -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName 'Cargo.toml') } |
        Select-Object -First 1
    if (-not $SourceRoot) { throw 'unexpected source payload layout' }
    $repoRoot = $SourceRoot.FullName

    $TargetRelease = Join-Path $repoRoot 'target\release'
    New-Item -ItemType Directory -Force -Path $TargetRelease | Out-Null
    Copy-Item (Join-Path $BootstrapDir 'bni-windows-x86_64.exe') (Join-Path $TargetRelease 'bni.exe')
    Copy-Item (Join-Path $BootstrapDir 'bnc-windows-x86_64.exe') (Join-Path $TargetRelease 'bnc.exe')
    Copy-Item (Join-Path $BootstrapDir 'bn_rt-windows-x86_64.lib') (Join-Path $TargetRelease 'bn_rt.lib')
    $NoBuild = $true
}
Set-Location $repoRoot

if (-not $NoBuild) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo (Rust 1.97+) is required to build; pass -NoBuild to install prebuilt binaries."
    }
    Write-Log "==> Building release binaries (cargo build --release --workspace --bins)"
    cargo build --release --workspace --bins
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    Write-Log "==> Building native runtime (cargo build -p bn_rt --release)"
    cargo build -p bn_rt --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p bn_rt failed" }
}

$bniBin = Join-Path $repoRoot 'target\release\bni.exe'
$bncBin = Join-Path $repoRoot 'target\release\bnc.exe'
$bnRtLib = Join-Path $repoRoot 'target\release\bn_rt.lib'
foreach ($bin in @($bniBin, $bncBin)) {
    if (-not (Test-Path $bin)) { throw "missing binary '$bin' (build first, or drop -NoBuild)" }
}
if (-not (Test-Path $bnRtLib)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Log "==> bn_rt.lib missing; building cargo -p bn_rt --release"
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

Write-Log "==> Installing to $Prefix"
foreach ($dir in @($binDir, $libDir, $modDir, $diagDir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}
Add-Content -Path $InstallLog -Value "    ensured dirs: $binDir $libDir $modDir $diagDir"

$bniDest = Join-Path $binDir 'bni.exe'
$bncDest = Join-Path $binDir 'bnc.exe'
$rtDest = Join-Path $libDir 'bn_rt.lib'
Copy-Item $bniBin $bniDest -Force; Record-Installed $bniDest
Copy-Item $bncBin $bncDest -Force; Record-Installed $bncDest
Copy-Item $bnRtLib $rtDest -Force; Record-Installed $rtDest

Get-ChildItem (Join-Path $repoRoot 'modules\bn\*.bn') | ForEach-Object {
    $dest = Join-Path $modDir $_.Name
    Copy-Item $_.FullName $dest -Force
    Record-Installed $dest
}

$diagSrc = Join-Path $repoRoot 'share\bn\diagnostics'
Get-ChildItem $diagSrc -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
    $rel = $_.FullName.Substring($diagSrc.Length).TrimStart('\')
    $dest = Join-Path $diagDir $rel
    New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
    Copy-Item $_.FullName $dest -Force
    Record-Installed $dest
}

Write-Log "==> Installed:"
Write-Log "    $bniDest, $bncDest"
Write-Log "    $rtDest"
Write-Log "    $modDir\, $diagDir\"

# Add the bin directory to the user's PATH (idempotent).
if (-not $NoPathUpdate) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $binDir) {
        $newPath = if ([string]::IsNullOrEmpty($userPath)) { $binDir } else { "$userPath;$binDir" }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Write-Log "==> Added $binDir to your user PATH (restart the shell to pick it up)."
    }
}

# Verify against the installed copy, from a clean working directory.
Write-Log "==> Verifying"
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
        Write-Log "    stdlib module resolution OK (BNMath.ABS(-7.0) = 7.0)"
    } else {
        Write-Warning "stdlib check did not return the expected value; output was: $out"
        Add-Content -Path $InstallLog -Value "    warning: stdlib check output: $out"
    }
} finally {
    Pop-Location
    Remove-Item -Recurse -Force $tmp
}

# --- uninstall script for this install (user profile) -------------------------
$nl = [Environment]::NewLine
$lines = New-Object System.Collections.Generic.List[string]
[void]$lines.Add("# Generated by Basic Next install.ps1 on $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')")
[void]$lines.Add("# Removes only the files recorded for Prefix=$Prefix")
[void]$lines.Add("# Matching install log: $InstallLog")
[void]$lines.Add('$ErrorActionPreference = ''Stop''')
[void]$lines.Add("Write-Host '==> Uninstalling Basic Next files under $Prefix'")
for ($i = $script:Manifest.Count - 1; $i -ge 0; $i--) {
    $p = $script:Manifest[$i]
    $q = $p.Replace("'", "''")
    [void]$lines.Add("if (Test-Path -LiteralPath '$q') { Remove-Item -LiteralPath '$q' -Force; Write-Host \"    removed $q\" }")
}
foreach ($d in @(
        $modDir,
        (Join-Path $Prefix 'share\bn\modules'),
        $diagDir,
        (Join-Path $Prefix 'share\bn')
    )) {
    $qd = $d.Replace("'", "''")
    [void]$lines.Add("if ((Test-Path -LiteralPath '$qd') -and -not (Get-ChildItem -LiteralPath '$qd' -Force -ErrorAction SilentlyContinue | Select-Object -First 1)) {")
    [void]$lines.Add("  Remove-Item -LiteralPath '$qd' -Force -ErrorAction SilentlyContinue")
    [void]$lines.Add("  Write-Host \"    removed empty $qd\"")
    [void]$lines.Add('}')
}
[void]$lines.Add("Write-Host '==> Uninstall finished.'")
[void]$lines.Add("Write-Host 'note: install.log under BN_STATE_DIR was kept; delete it manually if you want.'")
Set-Content -Path $UninstallScript -Value ($lines -join $nl) -Encoding UTF8

Write-Log "==> Wrote uninstall script: $UninstallScript"
Write-Log "==> Install log: $InstallLog"
Write-Log "==> Done."
Add-Content -Path $InstallLog -Value ("===== end install $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz') =====")
} finally {
    Set-Location $InitialLocation
    if ($BootstrapDir -and (Test-Path $BootstrapDir)) {
        Remove-Item -LiteralPath $BootstrapDir -Recurse -Force
    }
}
