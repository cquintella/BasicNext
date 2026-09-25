param(
    [Parameter(Mandatory = $true)]
    [string]$DistDir,

    [Parameter(Mandatory = $true)]
    [string]$RepositoryRoot
)

$ErrorActionPreference = 'Stop'
$DistDir = (Resolve-Path $DistDir).Path
$RepositoryRoot = (Resolve-Path $RepositoryRoot).Path
$Tag = 'v0.0.0-test'
$Scratch = Join-Path ([System.IO.Path]::GetTempPath()) ("basicnext-windows-installer-test-" + [guid]::NewGuid().ToString('N'))
$ReleaseDir = Join-Path $Scratch 'release'
$BootstrapDir = Join-Path $Scratch 'bootstrap'

function Invoke-IsolatedInstaller(
    [string]$PowerShellPath,
    [string]$Scenario,
    [bool]$ExpectSuccess
) {
    $Prefix = Join-Path $Scratch "prefix-$Scenario"
    $StateDir = Join-Path $Scratch "state-$Scenario"
    $PreviousVersion = $env:BN_VERSION
    $PreviousBase = $env:BN_RELEASE_BASE
    $PreviousState = $env:BN_STATE_DIR
    $UserPathBefore = [Environment]::GetEnvironmentVariable('Path', 'User')
    try {
        $env:BN_VERSION = $Tag
        $env:BN_RELEASE_BASE = $ReleaseDir
        $env:BN_STATE_DIR = $StateDir
        & $PowerShellPath -NoProfile -ExecutionPolicy Bypass -File $IsolatedInstaller `
            -Prefix $Prefix -NoPathUpdate
        $ExitCode = $LASTEXITCODE
    } finally {
        $env:BN_VERSION = $PreviousVersion
        $env:BN_RELEASE_BASE = $PreviousBase
        $env:BN_STATE_DIR = $PreviousState
    }

    $UserPathAfter = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($UserPathAfter -ne $UserPathBefore) {
        throw "$Scenario changed the user's PATH despite -NoPathUpdate"
    }

    if (-not $ExpectSuccess) {
        if ($ExitCode -eq 0) { throw "$Scenario accepted an asset with an invalid checksum" }
        if (Test-Path -LiteralPath $Prefix) { throw "$Scenario installed files after checksum failure" }
        return
    }
    if ($ExitCode -ne 0) { throw "$Scenario exited with $ExitCode" }

    foreach ($InstalledPath in @(
            'bin\bni.exe',
            'bin\bnc.exe',
            'lib\bn_rt.lib',
            'share\bn\modules\bn\BNMath.bn'
        )) {
        $FullPath = Join-Path $Prefix $InstalledPath
        if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
            throw "$Scenario did not create $InstalledPath"
        }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $StateDir 'uninstall.ps1') -PathType Leaf)) {
        throw "$Scenario did not create uninstall.ps1"
    }
}

try {
    New-Item -ItemType Directory -Force -Path $ReleaseDir, $BootstrapDir | Out-Null

    foreach ($AssetName in @(
            'bni-windows-x86_64.exe',
            'bnc-windows-x86_64.exe',
            'bn_rt-windows-x86_64.lib'
        )) {
        Copy-Item (Join-Path $DistDir $AssetName) (Join-Path $ReleaseDir $AssetName)
    }

    $Installer = Join-Path $ReleaseDir 'install.ps1'
    Copy-Item (Join-Path $RepositoryRoot 'scripts\install.ps1') $Installer
    $SourceArchive = Join-Path $ReleaseDir "basicnext-source-$Tag.zip"
    Push-Location $RepositoryRoot
    try {
        git archive --format=zip --prefix='BasicNext-0.0.0-test/' --output=$SourceArchive HEAD
        if ($LASTEXITCODE -ne 0) { throw 'git archive failed' }
    } finally {
        Pop-Location
    }

    $ChecksumLines = Get-ChildItem $ReleaseDir -File | Sort-Object Name | ForEach-Object {
        "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
    }
    Set-Content -Path (Join-Path $ReleaseDir 'SHA256SUMS') -Value $ChecksumLines -Encoding Ascii

    $IsolatedInstaller = Join-Path $BootstrapDir 'install.ps1'
    Copy-Item $Installer $IsolatedInstaller

    $BniAsset = Join-Path $ReleaseDir 'bni-windows-x86_64.exe'
    $BniBackup = Join-Path $Scratch 'bni-windows-x86_64.exe.clean'
    Copy-Item $BniAsset $BniBackup
    try {
        Add-Content -LiteralPath $BniAsset -Value 'tampered-after-checksum'
        Invoke-IsolatedInstaller 'powershell.exe' 'checksum-rejection' $false
    } finally {
        Copy-Item $BniBackup $BniAsset -Force
    }

    Invoke-IsolatedInstaller 'powershell.exe' 'windows-powershell-5' $true
    $PowerShell7 = (Get-Process -Id $PID).Path
    Invoke-IsolatedInstaller $PowerShell7 'powershell-7' $true

    Write-Host 'windows installer integration: OK'
} finally {
    if (Test-Path $Scratch) {
        Remove-Item -LiteralPath $Scratch -Recurse -Force
    }
}
