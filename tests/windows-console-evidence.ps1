param(
    [string]$Bn = ""
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
if ([Console]::IsOutputRedirected) {
    throw "Run this script directly in Windows Terminal or conhost; stdout must remain a TTY."
}
if (-not $Bn) {
    Push-Location $repo
    try {
        cargo build --locked
    } finally {
        Pop-Location
    }
    $Bn = Join-Path $repo "target\debug\bn.exe"
}
if (-not (Test-Path $Bn)) {
    throw "bn executable not found: $Bn"
}

$evidence = Join-Path $repo "done\project\windows-console-evidence.txt"
Start-Transcript -Path $evidence -Force
try {
    Write-Host "Host: $([System.Environment]::OSVersion.VersionString)"
    Write-Host "Terminal: $($Host.Name)"
    Write-Host "Initial RawUI window: $($Host.UI.RawUI.WindowSize.Width)x$($Host.UI.RawUI.WindowSize.Height)"
    & $Bn run (Join-Path $repo "tests\grammar\valid\console-size.bn")
    if ($LASTEXITCODE -ne 0) { throw "initial console-size failed: $LASTEXITCODE" }

    Read-Host "Resize the terminal, then press Enter"
    Write-Host "Resized RawUI window: $($Host.UI.RawUI.WindowSize.Width)x$($Host.UI.RawUI.WindowSize.Height)"
    & $Bn run (Join-Path $repo "tests\grammar\valid\console-size.bn")
    if ($LASTEXITCODE -ne 0) { throw "resized console-size failed: $LASTEXITCODE" }

    & $Bn run (Join-Path $repo "tests\grammar\valid\console-print-at.bn")
    if ($LASTEXITCODE -ne 0) { throw "in-bounds PrintAt failed: $LASTEXITCODE" }
    Write-Host "PrintAt(1, 1): exit 0"

    foreach ($fixture in @("console-print-at-column-oob.bn", "console-print-at-row-oob.bn")) {
        & $Bn run (Join-Path $repo "tests\grammar\valid\$fixture")
        if ($LASTEXITCODE -ne 1) { throw "$fixture returned $LASTEXITCODE instead of 1" }
        Write-Host "$fixture: exit 1 (expected INDEX_OUT_OF_BOUNDS above)"
    }

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Bn
    $start.Arguments = "run `"$(Join-Path $repo 'tests\grammar\valid\console-size.bn')`""
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $process = [System.Diagnostics.Process]::Start($start)
    $process.StandardInput.Close()
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw "stdin-pipe/stdout-TTY check failed: $($process.ExitCode)" }
    Write-Host "Piped stdin with inherited stdout TTY: exit 0"
} finally {
    Stop-Transcript
}

Write-Host "Evidence written to $evidence"
