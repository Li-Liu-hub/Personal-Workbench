param(
    [string]$TargetDir,
    [string]$OutputName = '个人办公台.exe'
)

$ErrorActionPreference = 'Stop'

if ([System.IO.Path]::GetFileName($OutputName) -ne $OutputName -or $OutputName -notmatch '\.exe$') {
    throw 'OutputName must be an executable file name without directory components.'
}

if (-not (Get-Command gcc.exe -ErrorAction SilentlyContinue)) {
    $mingwCandidates = @(
        $env:MSYS2_UCRT64_BIN,
        'D:\msys64\ucrt64\bin',
        'C:\msys64\ucrt64\bin'
    ) | Where-Object { $_ -and (Test-Path (Join-Path $_ 'gcc.exe')) }
    if ($mingwCandidates.Count -gt 0) {
        $env:Path = "$($mingwCandidates[0]);$env:Path"
    }
}

if (-not (Get-Command gcc.exe -ErrorAction SilentlyContinue)) {
    throw 'MinGW gcc.exe was not found. Install the MSYS2 UCRT64 toolchain and add its bin directory to PATH or MSYS2_UCRT64_BIN.'
}

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $tempDir = [System.IO.Path]::GetTempPath()
    $TargetDir = Join-Path $tempDir 'personal-desk-target'
}

if ($TargetDir -match '[^\x00-\x7F]') {
    throw "Cargo target path must use ASCII characters for the Windows GNU linker. Choose an ASCII path and pass it with -TargetDir."
}

$env:CARGO_TARGET_DIR = $TargetDir

cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo build --release --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$executable = Join-Path $TargetDir 'release\personal-desk.exe'
$distDir = Join-Path $PSScriptRoot '..\dist'
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
$portableExecutable = Join-Path $distDir $OutputName
& (Join-Path $PSScriptRoot 'publish-update.ps1') -Executable $executable
$portableExecutable = [IO.Path]::GetFullPath($portableExecutable)
$running = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -eq $portableExecutable }).Count -gt 0
if ($running) {
    Write-Output "Update published. The running app can install it from the Update page: $portableExecutable"
} else {
    Copy-Item -LiteralPath $executable -Destination $portableExecutable -Force
    Write-Output "Build succeeded: $portableExecutable"
}
