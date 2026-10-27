param([string]$Destination = (Join-Path $PSScriptRoot '../.artifacts/resume/typst/typst-x86_64-pc-windows-msvc'))
$ErrorActionPreference = 'Stop'
$version = '0.15.1'
$expected = '19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736'
$work = Join-Path ([IO.Path]::GetTempPath()) ('personal-desk-typst-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work -Force | Out-Null
$archive = Join-Path $work 'typst.zip'
Invoke-WebRequest -Uri "https://github.com/typst/typst/releases/download/v$version/typst-x86_64-pc-windows-msvc.zip" -OutFile $archive
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Typst archive hash mismatch. Files were not installed.' }
Expand-Archive -LiteralPath $archive -DestinationPath $work
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
foreach ($name in @('typst.exe', 'LICENSE', 'NOTICE', 'README.md')) {
    Copy-Item -LiteralPath (Join-Path $work "typst-x86_64-pc-windows-msvc/$name") -Destination (Join-Path $Destination $name) -Force
}
Write-Output "Typst $version installed: $Destination"
Write-Output "Downloaded archive retained for inspection: $archive"
