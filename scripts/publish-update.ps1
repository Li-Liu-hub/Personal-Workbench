param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Notes,
    [string]$MetadataDirectory
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$metadataRoot = if ($MetadataDirectory) { (Resolve-Path -LiteralPath $MetadataDirectory).Path } else { $projectRoot }
$versionLine = Get-Content (Join-Path $metadataRoot 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^" ]+)"' } | Select-Object -First 1
if ($versionLine -notmatch '^version\s*=\s*"([^" ]+)"') { throw 'Package version not found' }
$releaseVersion = $Matches[1]
$binaryVersion = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
if ($binaryVersion -ne $releaseVersion) {
    throw "Executable version '$binaryVersion' does not match package version '$releaseVersion'. Build the current version before publishing."
}
if ([string]::IsNullOrWhiteSpace($Notes)) {
    $history = Get-Content -LiteralPath (Join-Path $metadataRoot 'assets/release_history.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $entry = @($history | Where-Object { $_.version -eq $releaseVersion })
    if ($entry.Count -ne 1 -or @($entry[0].changes).Count -eq 0) {
        throw "Release history must contain one nonempty entry for $releaseVersion"
    }
    $Notes = $entry[0].changes -join "`n"
}
$feedRoot = Join-Path $projectRoot 'dist/updates'
$manifestPath = Join-Path $feedRoot 'latest.json'
if (Test-Path -LiteralPath $manifestPath) {
    $published = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ([version]$published.version -gt [version]$releaseVersion) {
        throw "A newer version '$($published.version)' is already published."
    }
    if ($published.version -eq $releaseVersion -and
        $published.sha256 -ne (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant()) {
        throw "Version '$releaseVersion' already has a different executable. Increment the package version before publishing another change."
    }
}
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
Move-Item -LiteralPath $pending -Destination $manifestPath -Force
Write-Output "Published update: $releaseVersion"
