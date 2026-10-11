<#
.SYNOPSIS
    Builds a Basic Next program into an executable optimized for this machine.

.DESCRIPTION
    A thin wrapper over bnc: it asks for the highest optimization level and
    the instructions of the host CPU (bnc --opt 3 --cpu native, which passes
    -O3 and -march=native / -mcpu=native to clang), then removes the symbol
    table with llvm-strip. Linking, the runtime library and the Windows
    system libraries stay in bnc.

    The executable runs on this machine and on machines with the same CPU
    features; for a portable executable use plain bnc (--cpu generic). No
    option here changes a program's results: bnc never enables fast-math.

.PARAMETER Entry
    The program's entry file (.bn).

.PARAMETER Output
    The executable to write. Default: the entry name with .exe.

.PARAMETER KeepSymbols
    Do not strip the executable.

.PARAMETER BncOptions
    Further bnc options, passed through (for example --module-path <dir>).
    -g keeps the symbols, as a debug build needs them.

.EXAMPLE
    .\scripts\build-native.ps1 examples\two_files.bn
    .\scripts\build-native.ps1 -Output app.exe main.bn --module-path lib

.NOTES
    BNC         the bnc to run (default: bnc on PATH, else this checkout's
                target\release\bnc.exe or target\debug\bnc.exe)
    LLVM_STRIP  the strip tool (default: llvm-strip on PATH)
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Entry,
    [string]$Output = "",
    [switch]$KeepSymbols,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$BncOptions = @()
)

$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
    [Console]::Error.WriteLine("build-native: $Message")
    exit 1
}

function Find-Bnc {
    if ($env:BNC) { return $env:BNC }
    $onPath = Get-Command bnc -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($profile in @("release", "debug")) {
        $candidate = Join-Path $PSScriptRoot "..\target\$profile\bnc.exe"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return (Resolve-Path $candidate).Path }
    }
    Fail "bnc not found: install Basic Next (scripts\install.ps1) or set BNC"
}

function Find-Strip {
    if ($env:LLVM_STRIP) { return $env:LLVM_STRIP }
    $onPath = Get-Command llvm-strip -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    return $null
}

foreach ($option in $BncOptions) {
    if ($option -in @("--opt", "--cpu", "--target", "--emit", "-o", "--output")) {
        Fail "$option is fixed by this script (--opt 3 --cpu native, native executable; use -Output)"
    }
    if ($option -in @("-g", "--debug")) { $KeepSymbols = $true }
}

if (-not (Test-Path -LiteralPath $Entry -PathType Leaf)) { Fail "entry file not found: $Entry" }
if (-not $Output) { $Output = [IO.Path]::GetFileNameWithoutExtension($Entry) + ".exe" }

$bnc = Find-Bnc
# Native tools report on stderr; with "Stop", Windows PowerShell 5.1 would
# turn each redirected stderr line into a terminating error. Exit codes
# are checked explicitly below.
$ErrorActionPreference = "Continue"
$help = & $bnc --help 2>$null | Out-String
if ($help -notmatch "--cpu") {
    $version = & $bnc --version 2>$null | Out-String
    Fail "$bnc ($($version.Trim())) has no --cpu option; it needs Basic Next 0.6.5 or later (set BNC to choose another)"
}

& $bnc --opt 3 --cpu native @BncOptions -o $Output $Entry
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not $KeepSymbols) {
    $strip = Find-Strip
    if ($strip) {
        & $strip --strip-all $Output
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
        # Without -g, bnc writes no PDB; only the COFF symbol table remains.
        Write-Warning "llvm-strip not found; $Output keeps its symbol table (install LLVM or set LLVM_STRIP)"
    }
}

$size = (Get-Item -LiteralPath $Output).Length
$cpu = $env:PROCESSOR_ARCHITECTURE
Write-Output "${Output}: $size bytes, optimized for $cpu (Windows)"
