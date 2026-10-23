param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Notes = '百科全书扩充 Java 主流生态与关联练习，修复列表覆盖输入和筛选交互，保留既有数据与作答记录'
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
