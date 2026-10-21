param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Notes = '刷题与英语新增讲解追问侧栏、一键启动模型和真实速度统计；单词增加造句、双向翻译、语法及混合训练；新增Java项目修Bug与实际测试；修复联系人视图切换，统一联系人和物品列表；保留个人数据及历史记录'
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
