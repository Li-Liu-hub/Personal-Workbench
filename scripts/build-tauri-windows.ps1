$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$toolchain = 'stable-x86_64-pc-windows-msvc'
$installed = & rustup toolchain list
if ($LASTEXITCODE -ne 0 -or -not ($installed -match [regex]::Escape($toolchain))) {
    throw "Install the Rust toolchain '$toolchain' before building the Windows release."
}
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio Build Tools with the C++ x64 toolchain are required.' }
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Visual Studio C++ x64 Build Tools were not found.' }
$vcvars = Join-Path $installation 'VC/Auxiliary/Build/vcvars64.bat'
$lines = & cmd.exe /c ('call "' + $vcvars + '" >nul && set')
if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the Visual Studio x64 build environment.' }
foreach ($line in $lines) {
    $separator = $line.IndexOf('=')
    if ($separator -gt 0) {
        [Environment]::SetEnvironmentVariable($line.Substring(0, $separator), $line.Substring($separator + 1), 'Process')
    }
}
$env:RUSTUP_TOOLCHAIN = $toolchain
Push-Location (Join-Path $root 'tauri-migration')
try {
    & npx.cmd tauri build --no-bundle --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }
} finally {
    Pop-Location
}
$versionLine = Get-Content (Join-Path $root 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($versionLine -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'The application version is missing.' }
$version = $Matches[1]
$executable = Join-Path $root "tauri-migration/src-tauri/target/x86_64-pc-windows-msvc/release/PersonalDesk.exe"
if (-not (Test-Path -LiteralPath $executable)) { throw "The release EXE was not produced: $executable" }
$binaryVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
if ($binaryVersion -ne $version) { throw "The EXE version '$binaryVersion' does not match Cargo.toml '$version'." }
Write-Output "Built $executable ($version)"
