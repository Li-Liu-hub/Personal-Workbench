param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Notes = '修复日记与文档归组后看似消失的问题；同本图书内拖放不再另建图书，归组后自动打开原文档标签并显示图书名称；保留日记正文、标题、标签及日期关联'
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$versionLine = Get-Content (Join-Path $projectRoot 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($versionLine -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'Package version not found' }
$releaseVersion = $Matches[1]
$feedRoot = Join-Path $projectRoot 'dist/updates'
$releaseRoot = Join-Path $feedRoot $releaseVersion
New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
$payload = Join-Path $releaseRoot 'PersonalDesk.exe'
Copy-Item -LiteralPath $Executable -Destination $payload -Force
$manifest = [ordered]@{
    app = 'personal-desk'
    version = $releaseVersion
    executable = "$releaseVersion/PersonalDesk.exe"
    sha256 = (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash.ToLowerInvariant()
    size = (Get-Item -LiteralPath $payload).Length
    notes = $Notes
}
$pending = Join-Path $feedRoot 'latest.pending.json'
[IO.File]::WriteAllText($pending, ($manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $pending -Destination (Join-Path $feedRoot 'latest.json') -Force
Write-Output "Published update: $releaseVersion"
