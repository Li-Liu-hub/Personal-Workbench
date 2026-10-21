param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Notes = '新增刷题王与5040词IELTS练习、本地Spark模型批改、双记录本及掌握降频；恢复简约台内写作、日期日志、Markdown预览；移除大模型聊天页面，保留已有数据和导出'
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
