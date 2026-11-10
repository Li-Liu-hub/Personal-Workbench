param(
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$webRoot = Join-Path $projectRoot 'web/markdown-editor'
$assetRoot = Join-Path $projectRoot 'assets/markdown-editor'
$bundlePath = Join-Path $assetRoot 'editor-bundle.zip'
$temporaryBundle = Join-Path $projectRoot '.markdown-editor-bundle.tmp.zip'

Push-Location $webRoot
try {
    if (-not $SkipInstall) {
        npm ci --ignore-scripts --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Markdown editor dependency installation failed.' }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Markdown editor build failed.' }
} finally {
    Pop-Location
}

$licensePackages = @('codemirror', '@marijn/find-cluster-break', 'crelt', 'style-mod', 'w3c-keyname')
foreach ($scope in @('@codemirror', '@lezer')) {
    $scopeRoot = Join-Path (Join-Path $webRoot 'node_modules') $scope
    if (Test-Path -LiteralPath $scopeRoot) {
        $licensePackages += Get-ChildItem -LiteralPath $scopeRoot -Directory | ForEach-Object {
            "$scope/$($_.Name)"
        }
    }
}
$notice = [Collections.Generic.List[string]]::new()
$notice.Add('Third-party notices for the bundled Markdown editor')
$notice.Add('')
foreach ($package in $licensePackages) {
    $packageRoot = Join-Path (Join-Path $webRoot 'node_modules') $package
    $licensePath = Join-Path $packageRoot 'LICENSE'
    if (-not (Test-Path -LiteralPath $licensePath)) {
        $licensePath = Join-Path $packageRoot 'LICENSE.md'
    }
    if (-not (Test-Path -LiteralPath $licensePath)) {
        throw "Missing license file for bundled package: $package"
    }
    $notice.Add("----- $package -----")
    $notice.Add((Get-Content -LiteralPath $licensePath -Raw -Encoding UTF8).Trim())
    $notice.Add('')
}
[IO.File]::WriteAllText(
    (Join-Path $assetRoot 'THIRD-PARTY-NOTICES.txt'),
    ($notice -join "`r`n"),
    [Text.UTF8Encoding]::new($false)
)

Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path -LiteralPath $temporaryBundle) { Remove-Item -LiteralPath $temporaryBundle -Force }
if (Test-Path -LiteralPath $bundlePath) { Remove-Item -LiteralPath $bundlePath -Force }
[IO.Compression.ZipFile]::CreateFromDirectory(
    $assetRoot,
    $temporaryBundle,
    [IO.Compression.CompressionLevel]::Optimal,
    $false
)
Move-Item -LiteralPath $temporaryBundle -Destination $bundlePath -Force
Write-Output "Built offline editor bundle: $bundlePath"
