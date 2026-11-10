param([string]$TargetDir)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
$installation = $null
if (Test-Path -LiteralPath $vswhere) {
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $installation) {
    $candidates = @(
        (Join-Path $programFilesX86 'Microsoft Visual Studio/2022/BuildTools'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio/2022/BuildTools'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio/2022/Community'),
        (Join-Path $programFilesX86 'Microsoft Visual Studio/2022/Community')
    ) | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'VC/Auxiliary/Build/vcvars64.bat') }
    if (@($candidates).Count -gt 0) { $installation = @($candidates)[0] }
}
if (-not $installation) { throw 'Visual Studio C++ x64 Build Tools were not found.' }
$vcvars = Join-Path $installation 'VC/Auxiliary/Build/vcvars64.bat'
$lines = & cmd.exe /c ('call "' + $vcvars + '" >nul && set')
if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the Visual Studio x64 build environment.' }
foreach ($line in $lines) {
    $separator = $line.IndexOf('=')
    if ($separator -gt 0) {
        [Environment]::SetEnvironmentVariable($line.Substring(0, $separator), $line.Substring($separator + 1), 'Process')
    }
}
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'
if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path (Split-Path -Parent $root) 'personal-desk-target'
}
$env:CARGO_TARGET_DIR = $TargetDir

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

    $client = Join-Path $root 'tauri-migration'
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
    } finally {
        Pop-Location
    }
    & cargo.exe clippy --locked --release --manifest-path (Join-Path $client 'src-tauri/Cargo.toml') --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Tauri backend lint check failed.' }

    & python -m unittest discover -s scripts/tests -v
    if ($LASTEXITCODE -ne 0) { throw 'Python regression tests failed.' }
    Write-Output 'All standard checks passed.'
} finally {
    Pop-Location
}
