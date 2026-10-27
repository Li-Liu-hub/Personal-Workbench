param([string]$Executable = (Join-Path $PSScriptRoot '../dist/个人办公台.exe'))
$ErrorActionPreference = 'Stop'
$program = (Resolve-Path -LiteralPath $Executable).Path
if ([version](Get-Item -LiteralPath $program).VersionInfo.ProductVersion -lt [version]'0.15.18') {
    throw '请先将原程序入口更新到支持 MCP 的版本。'
}
$cli = Get-Command codex -ErrorAction Stop
$configDirectory = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }
$configPath = Join-Path $configDirectory 'config.toml'
$originalConfiguration = ''
if (Test-Path -LiteralPath $configPath) {
    $originalConfiguration = [IO.File]::ReadAllText($configPath)
    $backupDirectory = Join-Path $configDirectory 'backups'
    New-Item -ItemType Directory -Path $backupDirectory -Force | Out-Null
    Copy-Item -LiteralPath $configPath -Destination (Join-Path $backupDirectory ('before-hiring-mcp-'+[guid]::NewGuid().ToString('N')+'.toml'))
}
& $cli.Source mcp add personaldesk_hiring -- $program --mcp
if ($LASTEXITCODE -ne 0) { throw 'Codex MCP 注册失败；原配置备份已保留。' }
$configuration = [IO.File]::ReadAllText($configPath)
$section = [regex]::Match($configuration, '(?ms)^\[mcp_servers\.personaldesk_hiring\]\r?\n(?<body>.*?)(?=^\[|\z)')
if (!$section.Success) { throw '注册结果缺少求职 MCP 配置段。' }
$body = [regex]::Replace($section.Groups['body'].Value, '(?m)^(startup_timeout_sec|tool_timeout_sec)\s*=.*\r?\n?', '')
$replacement = "[mcp_servers.personaldesk_hiring]`nstartup_timeout_sec = 15`ntool_timeout_sec = 120`n" + $body
# Keep the user's original formatting, comments and other server sections. The CLI
# may normalize unrelated options (for example removing an empty args array).
$originalSection = [regex]::Match($originalConfiguration, '(?ms)^\[mcp_servers\.personaldesk_hiring\]\r?\n.*?(?=^\[|\z)')
if ($originalSection.Success) {
    $configuration = $originalConfiguration.Substring(0, $originalSection.Index) + $replacement + $originalConfiguration.Substring($originalSection.Index + $originalSection.Length)
} else {
    $configuration = $originalConfiguration.TrimEnd() + "`n`n" + $replacement
}
$pending = Join-Path $configDirectory ('config-hiring-'+[guid]::NewGuid().ToString('N')+'.pending')
[IO.File]::WriteAllText($pending, $configuration, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($pending, $configPath, $true)
& $cli.Source mcp get personaldesk_hiring --json
if ($LASTEXITCODE -ne 0) { throw '无法读取已注册的求职 MCP 服务。' }
