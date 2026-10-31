param(
    [string]$Bni = ""
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
if ([Console]::IsOutputRedirected) {
    throw "Run this script directly in Windows Terminal or conhost; stdout must remain a TTY."
}
if (-not $Bni) {
    # The one build of the test prerequisites, through Git Bash (the `bash`
    # that PowerShell finds first may be the WSL launcher in System32).
    $gitBash = Join-Path $env:ProgramFiles "Git\bin\bash.exe"
    Push-Location $repo
    try {
        & $gitBash scripts/build-test-prereqs.sh
        if ($LASTEXITCODE -ne 0) { throw "build-test-prereqs.sh failed: $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    $Bni = Join-Path $repo "target\debug\bni.exe"
}
if (-not (Test-Path $Bni)) {
    throw "bni executable not found: $Bni"
}

$evidence = Join-Path $repo "target\windows-console-evidence.txt"
Start-Transcript -Path $evidence -Force
try {
    Write-Host "Host: $([System.Environment]::OSVersion.VersionString)"
    Write-Host "Terminal: $($Host.Name)"
    Write-Host "Initial RawUI window: $($Host.UI.RawUI.WindowSize.Width)x$($Host.UI.RawUI.WindowSize.Height)"
    & $Bni run (Join-Path $repo "tests\grammar\valid\console-size.bn")
    if ($LASTEXITCODE -ne 0) { throw "initial console-size failed: $LASTEXITCODE" }

    Read-Host "Resize the terminal, then press Enter"
    Write-Host "Resized RawUI window: $($Host.UI.RawUI.WindowSize.Width)x$($Host.UI.RawUI.WindowSize.Height)"
    & $Bni run (Join-Path $repo "tests\grammar\valid\console-size.bn")
    if ($LASTEXITCODE -ne 0) { throw "resized console-size failed: $LASTEXITCODE" }

    & $Bni run (Join-Path $repo "tests\grammar\valid\console-print-at.bn")
    if ($LASTEXITCODE -ne 0) { throw "in-bounds PrintAt failed: $LASTEXITCODE" }
    Write-Host "PrintAt(1, 1): exit 0"

    foreach ($fixture in @("console-print-at-column-oob.bn", "console-print-at-row-oob.bn")) {
        & $Bni run (Join-Path $repo "tests\grammar\valid\$fixture")
        if ($LASTEXITCODE -ne 1) { throw "$fixture returned $LASTEXITCODE instead of 1" }
        Write-Host "$fixture: exit 1 (expected INDEX_OUT_OF_BOUNDS above)"
    }

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Bni
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
