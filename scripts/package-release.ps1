param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../dist/release'),
    [string]$Compiler,
    [string]$TypstDirectory = (Join-Path $PSScriptRoot '../.artifacts/resume/typst/typst-x86_64-pc-windows-msvc'),
    [string]$MetadataDirectory
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$metadataRoot = if ($MetadataDirectory) { (Resolve-Path -LiteralPath $MetadataDirectory).Path } else { $root }
$line = Get-Content (Join-Path $metadataRoot 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($line -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'Missing package version' }
$version = $Matches[1]
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$binaryVersion = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
if ($binaryVersion -ne $version) {
    throw "Executable version '$binaryVersion' does not match package version '$version'. Build the current version before packaging."
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$portable = Join-Path $OutputDirectory 'PersonalDesk.exe'
if ($Executable -ne $portable) { Copy-Item -LiteralPath $Executable -Destination $portable -Force }
$readme = Join-Path $OutputDirectory '使用说明.txt'
$text = @"
个人办公台 $version · Windows x64

免安装：直接打开 PersonalDesk.exe。
安装版：打开 PersonalDesk-$version-Setup-x64.exe，按向导选择程序和数据目录。
已有数据：保留原 personal-desk-data-path.txt，或在安装向导选择原数据目录。
默认数据目录：%LOCALAPPDATA%\PersonalDesk。
游戏会保存棋盘、关卡和成绩。设置中可开启/关闭开机自启动。
本地模型和模型运行时不随客户端打包，客户端本身可独立使用。
简历功能的 Typst 引擎如随包提供，位于 tools/typst；个人简历资料不随程序包提供。
导出全部数据可备份记录及办公台文档；快捷路径只保存地址。
更新只替换程序；安装包不会包含个人账号、文档或模型文件。

源码与发布：https://github.com/Li-Liu-hub/Personal-Workbench
"@
[IO.File]::WriteAllText($readme,$text,[Text.UTF8Encoding]::new($false))
$portableZip = Join-Path $OutputDirectory "PersonalDesk-$version-Windows-x64.zip"
$portableFiles = @($portable, $readme)
$hiringLicense = Join-Path $metadataRoot 'assets/hiring/BossHunter-LICENSE.txt'
if (Test-Path -LiteralPath $hiringLicense) {
    $licenseDirectory = Join-Path $OutputDirectory 'licenses'
    New-Item -ItemType Directory -Path $licenseDirectory -Force | Out-Null
    Copy-Item -LiteralPath $hiringLicense -Destination (Join-Path $licenseDirectory 'BossHunter-LICENSE.txt') -Force
    $portableFiles += $licenseDirectory
}
$typstFiles = $null
if (Test-Path -LiteralPath (Join-Path $TypstDirectory 'typst.exe')) {
    $typstFiles = Join-Path $OutputDirectory 'tools/typst'
    New-Item -ItemType Directory -Path $typstFiles -Force | Out-Null
    foreach ($name in @('typst.exe', 'LICENSE', 'NOTICE', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $TypstDirectory $name) -Destination (Join-Path $typstFiles $name) -Force
    }
    $portableFiles += (Join-Path $OutputDirectory 'tools')
}
Compress-Archive -LiteralPath $portableFiles -DestinationPath $portableZip -Force
if ($Compiler) {
    $compilerArgs = @("/DAppVersion=$version", "/DAppExe=$portable", "/DOutputDir=$OutputDirectory")
    if ($typstFiles) { $compilerArgs += "/DTypstDir=$typstFiles" }
    & $Compiler @compilerArgs (Join-Path $root 'packaging/windows/setup.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
}
$assets = @($portable,$portableZip)
$setup = Join-Path $OutputDirectory "PersonalDesk-$version-Setup-x64.exe"
if (Test-Path -LiteralPath $setup) { $assets += $setup }
$checksums = $assets | ForEach-Object { "{0}  {1}" -f (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant(),[IO.Path]::GetFileName($_) }
[IO.File]::WriteAllLines((Join-Path $OutputDirectory 'SHA256SUMS.txt'),$checksums,[Text.UTF8Encoding]::new($false))
Write-Output "Release files: $OutputDirectory"
