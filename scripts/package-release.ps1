param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../dist/release'),
    [string]$Compiler,
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
小游戏、刷题王、每日单词和百科全书由 D:\MyProject 下的独立程序提供，继续沿用原数据目录。设置中可开启/关闭开机自启动。
本地模型和模型运行时不随客户端打包，客户端本身可独立使用。
导出全部数据可备份个人办公台记录及文档；小工具和快捷路径只保存 EXE 或文件地址。
更新只替换程序；安装包不会包含个人账号、文档或模型文件。

源码与发布：https://github.com/Li-Liu-hub/Personal-Workbench
"@
[IO.File]::WriteAllText($readme,$text,[Text.UTF8Encoding]::new($false))
$portableZip = Join-Path $OutputDirectory "PersonalDesk-$version-Windows-x64.zip"
$portableFiles = @($portable, $readme)
Compress-Archive -LiteralPath $portableFiles -DestinationPath $portableZip -Force
if ($Compiler) {
    $compilerArgs = @("/DAppVersion=$version", "/DAppExe=$portable", "/DOutputDir=$OutputDirectory")
    & $Compiler @compilerArgs (Join-Path $root 'packaging/windows/setup.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
}
$assets = @($portable,$portableZip)
$setup = Join-Path $OutputDirectory "PersonalDesk-$version-Setup-x64.exe"
if (Test-Path -LiteralPath $setup) { $assets += $setup }
$checksums = $assets | ForEach-Object { "{0}  {1}" -f (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant(),[IO.Path]::GetFileName($_) }
[IO.File]::WriteAllLines((Join-Path $OutputDirectory 'SHA256SUMS.txt'),$checksums,[Text.UTF8Encoding]::new($false))
$legacyTypstDirectory = [IO.Path]::GetFullPath((Join-Path $root 'dist/release/tools/typst'))
$releaseRoot = [IO.Path]::GetFullPath((Join-Path $root 'dist/release'))
if ([StringComparer]::OrdinalIgnoreCase.Equals($OutputDirectory, $releaseRoot)) {
    $keepNames = @("PersonalDesk-$version-Windows-x64.zip", "PersonalDesk-$version-Setup-x64.exe")
    Get-ChildItem -LiteralPath $releaseRoot -File | Where-Object {
        ($_.Name -like 'PersonalDesk-*-Windows-x64.zip' -or $_.Name -like 'PersonalDesk-*-Setup-x64.exe') -and
        $_.Name -notin $keepNames
    } | ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force }
}
if ($legacyTypstDirectory.StartsWith($releaseRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and
    (Test-Path -LiteralPath $legacyTypstDirectory)) {
    Remove-Item -LiteralPath $legacyTypstDirectory -Recurse -Force
}
Write-Output "Release files: $OutputDirectory"
