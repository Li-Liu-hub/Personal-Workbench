param(
    [string]$Executable = (Join-Path $PSScriptRoot '../dist/release/PersonalDesk.exe'),
    [string]$AssetsDirectory = (Join-Path $PSScriptRoot '../dist/release'),
    [string]$MetadataDirectory,
    [string]$Tag,
    [string]$Title,
    [string]$Repository = 'Li-Liu-hub/Personal-Workbench'
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$metadataRoot = if ($MetadataDirectory) { (Resolve-Path -LiteralPath $MetadataDirectory).Path } else { $projectRoot }
$line = Get-Content (Join-Path $metadataRoot 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($line -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'Missing package version' }
$version = $Matches[1]
$Executable = (Resolve-Path -LiteralPath $Executable).Path
if ([IO.Path]::GetFileName($Executable) -ne 'PersonalDesk.exe') {
    throw 'The published asset must be named PersonalDesk.exe so the update manifest can point at it.'
}
$binaryVersion = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
if ($binaryVersion -ne $version) {
    throw "Executable version '$binaryVersion' does not match package version '$version'. Build the current version first."
}
$feedPath = Join-Path $projectRoot 'dist/updates/latest.json'
if (-not (Test-Path -LiteralPath $feedPath)) { throw 'Run scripts/publish-update.ps1 before publishing to GitHub.' }
$feed = Get-Content -LiteralPath $feedPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($feed.version -ne $version) {
    throw "The local update feed is $($feed.version); run scripts/publish-update.ps1 for $version first."
}
$hash = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($feed.sha256 -ne $hash -or [int64]$feed.size -ne (Get-Item -LiteralPath $Executable).Length) {
    throw 'The local update feed does not describe this executable. Republish the update feed first.'
}
if (-not $Tag) { $Tag = "v$version" }
if ($Tag -ne "v$version") { throw "Tag '$Tag' must match package version '$version'." }
if (-not $Title) {
    $history = Get-Content -LiteralPath (Join-Path $metadataRoot 'assets/release_history.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $entry = @($history | Where-Object { $_.version -eq $version })
    $Title = if ($entry.Count -eq 1 -and $entry[0].title) { "v$version · $($entry[0].title)" } else { "v$version" }
}
$AssetsDirectory = [IO.Path]::GetFullPath($AssetsDirectory)
$assets = @($Executable)
foreach ($name in @("PersonalDesk-$version-Windows-x64.zip", "PersonalDesk-$version-Setup-x64.exe", 'SHA256SUMS.txt')) {
    $path = Join-Path $AssetsDirectory $name
    if (Test-Path -LiteralPath $path) { $assets += $path }
}
$staging = Join-Path ([IO.Path]::GetTempPath()) ("personal-desk-release-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $staging -Force | Out-Null
$manifest = [ordered]@{
    app        = 'personal-desk'
    version    = $version
    executable = "https://github.com/$Repository/releases/download/$Tag/PersonalDesk.exe"
    sha256     = $hash
    size       = (Get-Item -LiteralPath $Executable).Length
    notes      = $feed.notes
}
$manifestPath = Join-Path $staging 'latest.json'
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
$assets += $manifestPath
$notesPath = Join-Path $staging 'notes.md'
[IO.File]::WriteAllText($notesPath, $feed.notes, [Text.UTF8Encoding]::new($false))
$commit = (git -C $projectRoot rev-parse HEAD).Trim()
$existing = (git -C $projectRoot tag --list $Tag)
if ($existing) {
    $tagCommit = (git -C $projectRoot rev-list -n 1 $Tag).Trim()
    if ($tagCommit -ne $commit) { throw "Tag $Tag points at $tagCommit, not the current commit $commit." }
} else {
    git -C $projectRoot tag -a $Tag -m "Release $version"
    if ($LASTEXITCODE -ne 0) { throw 'Failed to create the release tag.' }
}
git -C $projectRoot push origin $Tag 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to push the release tag.' }
gh release view $Tag --repo $Repository --json isDraft *> $null
if ($LASTEXITCODE -eq 0) {
    gh release edit $Tag --repo $Repository --title $Title --notes-file $notesPath --latest
    if ($LASTEXITCODE -ne 0) { throw 'Failed to update the GitHub release.' }
} else {
    gh release create $Tag --repo $Repository --title $Title --notes-file $notesPath --latest
    if ($LASTEXITCODE -ne 0) { throw 'Failed to create the GitHub release.' }
}
gh release upload $Tag --repo $Repository @assets --clobber
if ($LASTEXITCODE -ne 0) { throw 'Failed to upload the release assets.' }
$published = gh release view $Tag --repo $Repository --json isDraft,assets | ConvertFrom-Json
if ($published.isDraft) { throw "Release $Tag is still a draft." }
$uploaded = @($published.assets | ForEach-Object { $_.name })
foreach ($required in @('PersonalDesk.exe', 'latest.json')) {
    if ($uploaded -notcontains $required) { throw "Release $Tag is missing $required." }
}
$feedUrl = "https://github.com/$Repository/releases/latest/download/latest.json"
$remoteOk = $false
for ($attempt = 1; $attempt -le 10; $attempt++) {
    try {
        $remote = Invoke-RestMethod -Uri $feedUrl -TimeoutSec 20 -Headers @{ 'Cache-Control' = 'no-cache' }
        if ($remote.version -eq $version -and $remote.sha256 -eq $hash) { $remoteOk = $true; break }
    } catch {
    }
    Start-Sleep -Seconds 6
}
if (-not $remoteOk) { throw "The public feed $feedUrl does not serve $version yet." }
Remove-Item -LiteralPath $staging -Recurse -Force
Write-Output "Published GitHub release: $Tag ($version)"
Write-Output "Feed: $feedUrl"
Write-Output "Assets: $($uploaded -join ', ')"
