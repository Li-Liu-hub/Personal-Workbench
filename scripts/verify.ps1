param([string]$TargetDir)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $PSScriptRoot 'windows-build-env.ps1')

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path (Split-Path -Parent $root) 'personal-desk-target'
}
$previousTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $TargetDir
$client = Join-Path $root 'desktop'

Push-Location $root
try {
    & cargo.exe fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    & cargo.exe clippy --locked --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint check failed.' }
    & cargo.exe test --locked --workspace --all-targets --no-fail-fast
    if ($LASTEXITCODE -ne 0) { throw 'Rust regression tests failed.' }
    & cargo.exe clippy --locked --release --lib -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Release-only Rust lint check failed.' }
    & cargo.exe fmt --manifest-path (Join-Path $client 'src-tauri/Cargo.toml') --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Desktop backend formatting check failed.' }

    if (-not (Test-Path -LiteralPath (Join-Path $client 'node_modules'))) {
        Push-Location $client
        try {
            & npm.cmd ci
            if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
        } finally {
            Pop-Location
        }
    }
    Push-Location $client
    try {
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
        & npm.cmd run lint
        if ($LASTEXITCODE -ne 0) { throw 'Frontend lint failed.' }
        & npm.cmd run test
        if ($LASTEXITCODE -ne 0) { throw 'Frontend regression tests failed.' }
    } finally {
        Pop-Location
    }
    & cargo.exe clippy --locked --release --manifest-path (Join-Path $client 'src-tauri/Cargo.toml') --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Desktop backend lint check failed.' }

    & python -m unittest discover -s scripts/tests -v
    if ($LASTEXITCODE -ne 0) { throw 'Python regression tests failed.' }
    Write-Output 'All standard checks passed.'
} finally {
    Pop-Location
    # 不要把构建目录泄漏给调用方（build.ps1 之后还要用同一目录构建 Tauri 程序）。
    $env:CARGO_TARGET_DIR = $previousTargetDir
}