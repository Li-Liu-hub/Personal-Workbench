param(
    [string]$OutputName = '个人办公台.exe'
)

$ErrorActionPreference = 'Stop'

if ([System.IO.Path]::GetFileName($OutputName) -ne $OutputName -or $OutputName -notmatch '\.exe$') {
    throw 'OutputName must be an executable file name without directory components.'
}

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$client = Join-Path $root 'tauri-migration'

if (-not (Test-Path -LiteralPath (Join-Path $client 'node_modules'))) {
    Push-Location $client
    try {
        & npm.cmd ci
        if ($LASTEXITCODE -ne 0) { throw 'Installing the Tauri frontend dependencies failed.' }
    } finally {
        Pop-Location
    }
}

Push-Location $root
try {
    & cargo.exe fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'cargo fmt check failed.' }
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot 'build-tauri-windows.ps1')

$executable = Join-Path $root 'tauri-migration/src-tauri/target/x86_64-pc-windows-msvc/release/PersonalDesk.exe'
if (-not (Test-Path -LiteralPath $executable)) {
    throw "The Tauri release EXE was not produced: $executable"
}

$webView2Required = $false
$cargoLock = Get-Content -LiteralPath (Join-Path $root 'Cargo.lock') -Raw
if ($cargoLock -match '(?ms)^\[\[package\]\]\s*name = "webview2-com-sys"\s*version = "([^"]+)"') {
    $webView2Required = $true
    $webView2Version = $Matches[1]
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $registrySource = Join-Path $cargoHome 'registry\src'
    $loaderCandidates = @()
    if (Test-Path -LiteralPath $registrySource) {
        foreach ($registry in Get-ChildItem -LiteralPath $registrySource -Directory) {
            $candidate = Join-Path $registry.FullName "webview2-com-sys-$webView2Version\x64\WebView2Loader.dll"
            if (Test-Path -LiteralPath $candidate) { $loaderCandidates += $candidate }
        }
    }
    if ($loaderCandidates.Count -eq 0) {
        throw "WebView2Loader.dll for webview2-com-sys $webView2Version was not found in the Cargo cache."
    }
    $loaderHashes = @($loaderCandidates | ForEach-Object {
        (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash
    } | Sort-Object -Unique)
    if ($loaderHashes.Count -ne 1) { throw 'Cargo caches contain conflicting WebView2Loader.dll files.' }
    $loaderInBuild = Join-Path (Split-Path -Parent $executable) 'WebView2Loader.dll'
    Copy-Item -LiteralPath $loaderCandidates[0] -Destination $loaderInBuild -Force
}

$distDir = Join-Path $root 'dist'
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
$portableExecutable = Join-Path $distDir $OutputName
$distLoader = Join-Path $distDir 'WebView2Loader.dll'
if ($webView2Required) {
    Copy-Item -LiteralPath $loaderInBuild -Destination $distLoader -Force
}

& (Join-Path $PSScriptRoot 'publish-update.ps1') -Executable $executable
$portableExecutable = [IO.Path]::GetFullPath($portableExecutable)
$running = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -eq $portableExecutable }).Count -gt 0
if ($running) {
    Write-Output "Update published. The running app can install it from the Update page: $portableExecutable"
} else {
    Copy-Item -LiteralPath $executable -Destination $portableExecutable -Force
    Write-Output "Build succeeded: $portableExecutable"
}
