$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $PSScriptRoot 'windows-build-env.ps1')

# 构建目录：优先沿用调用方（build.ps1/verify.ps1）设置的 CARGO_TARGET_DIR，
# 否则使用 Tauri 工程自己的 target。二者必须与最后校验 EXE 的路径一致。
$targetRoot = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $root 'desktop/src-tauri/target'
} else {
    $env:CARGO_TARGET_DIR
}
$executable = Join-Path $targetRoot 'x86_64-pc-windows-msvc/release/PersonalDesk.exe'
# 先删除旧产物，避免把上一次构建的 EXE 当成这次的结果通过版本校验。
if (Test-Path -LiteralPath $executable) { Remove-Item -LiteralPath $executable -Force }

Push-Location (Join-Path $root 'desktop')
try {
    & npx.cmd tauri build --no-bundle --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }
} finally {
    Pop-Location
}
$versionLine = Get-Content (Join-Path $root 'desktop/src-tauri/Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($versionLine -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'The application version is missing.' }
$version = $Matches[1]
if (-not (Test-Path -LiteralPath $executable)) { throw "The release EXE was not produced: $executable" }
$binaryVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
if ($binaryVersion -ne $version) { throw "The EXE version '$binaryVersion' does not match Cargo.toml '$version'." }
Write-Output "Built $executable ($version)"