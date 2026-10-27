param([string]$TargetDir)

$ErrorActionPreference = 'Stop'
$previousPath = $env:Path
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo/bin'
        if (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) {
            $env:Path = "$cargoBin;$env:Path"
        }
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'Rust cargo is required.'
    }
    if (-not (Get-Command gcc.exe -ErrorAction SilentlyContinue)) {
        foreach ($candidate in @($env:MSYS2_UCRT64_BIN, 'D:/msys64/ucrt64/bin', 'C:/msys64/ucrt64/bin')) {
            if ($candidate -and (Test-Path -LiteralPath (Join-Path $candidate 'gcc.exe'))) {
                $env:Path = "$candidate;$env:Path"
                break
            }
        }
    }
    if ([string]::IsNullOrWhiteSpace($TargetDir)) {
        $TargetDir = Join-Path ([IO.Path]::GetTempPath()) 'personal-desk-target'
    }
    if ($TargetDir -match '[^\x00-\x7F]') {
        throw 'Use an ASCII target directory for the Windows GNU linker.'
    }
    $env:CARGO_TARGET_DIR = $TargetDir
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    cargo clippy --locked --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint check failed.' }
    cargo test --locked --workspace --all-targets --no-fail-fast
    if ($LASTEXITCODE -ne 0) { throw 'Rust regression tests failed.' }
    python -m unittest discover -s scripts/tests -v
    if ($LASTEXITCODE -ne 0) { throw 'Python regression tests failed.' }
    Write-Output 'All standard checks passed. Environment-dependent ignored tests are reported separately.'
} finally {
    $env:Path = $previousPath
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
