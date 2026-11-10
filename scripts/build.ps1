param(
    [string]$OutputName = '个人办公台.exe',
    [string]$TargetDir
)

$ErrorActionPreference = 'Stop'

if ([System.IO.Path]::GetFileName($OutputName) -ne $OutputName -or $OutputName -notmatch '\.exe$') {
    throw 'OutputName must be an executable file name without directory components.'
}

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$client = Join-Path $root 'desktop'
. (Join-Path $PSScriptRoot 'windows-build-env.ps1')

if (-not (Test-Path -LiteralPath (Join-Path $client 'node_modules'))) {
    Push-Location $client
    try {
        & npm.cmd ci
        if ($LASTEXITCODE -ne 0) { throw 'Installing the Tauri frontend dependencies failed.' }
    } finally {
        Pop-Location
    }
}

# 版本身份（格式、零告警、Rust 回归、前端构建/检查/回归、Python 回归）全部通过后才构建发布程序。
if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path (Split-Path -Parent $root) 'personal-desk-target'
}
& (Join-Path $PSScriptRoot 'verify.ps1') -TargetDir $TargetDir
if ($LASTEXITCODE -ne 0) { throw 'Verification failed; the release build was not started.' }

# 复用刚通过的构建目录，保证发布的 EXE 与校验过的依赖出自同一次构建。
$env:CARGO_TARGET_DIR = $TargetDir
& (Join-Path $PSScriptRoot 'build-tauri-windows.ps1')

$executable = Join-Path $TargetDir 'x86_64-pc-windows-msvc/release/PersonalDesk.exe'
if (-not (Test-Path -LiteralPath $executable)) {
    throw "The Tauri release EXE was not produced: $executable"
}

# WebView2 加载器：只有构建产物真的导入 WebView2Loader.dll 时才需要随程序分发。
# 依赖版本从 desktop 工程的锁文件读取（根核心库已不再依赖 webview2-com-sys）。
$binaryText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($executable))
$webView2Required = $binaryText.Contains('WebView2Loader.dll')
$loaderInBuild = Join-Path (Split-Path -Parent $executable) 'WebView2Loader.dll'
if ($webView2Required) {
    $desktopLockPath = Join-Path $root 'desktop/src-tauri/Cargo.lock'
    $desktopLock = Get-Content -LiteralPath $desktopLockPath -Raw
    if ($desktopLock -notmatch '(?ms)^\[\[package\]\]\s*name = "webview2-com-sys"\s*version = "([^"]+)"') {
        throw "The release EXE imports WebView2Loader.dll, but $desktopLockPath has no webview2-com-sys entry."
    }
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
    Copy-Item -LiteralPath $loaderCandidates[0] -Destination $loaderInBuild -Force
} elseif (Test-Path -LiteralPath $loaderInBuild) {
    # 程序已静态链接加载器，构建目录里的旧 DLL 不能混入本次产物。
    Remove-Item -LiteralPath $loaderInBuild -Force
}

$distDir = Join-Path $root 'dist'
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
$portableExecutable = Join-Path $distDir $OutputName
$distLoader = Join-Path $distDir 'WebView2Loader.dll'
if ($webView2Required) {
    Copy-Item -LiteralPath $loaderInBuild -Destination $distLoader -Force
} elseif (Test-Path -LiteralPath $distLoader) {
    Remove-Item -LiteralPath $distLoader -Force
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