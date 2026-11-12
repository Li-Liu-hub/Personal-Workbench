# 初始化 Windows MSVC 构建环境：cargo 所在目录、Rust MSVC 工具链与 Visual C++ x64 环境变量。
# 由其它脚本点源调用：. (Join-Path $PSScriptRoot 'windows-build-env.ps1')
# 只用点源，不要直接执行，否则环境变量只作用于子进程。
$ErrorActionPreference = 'Stop'

# 如果从 PowerShell 7 继承了 PSModulePath，Windows PowerShell 5.1 会误加载
# PS7 的 Utility 模块，导致 Get-FileHash 等 cmdlet 不可用。把 5.1 的模块目录
# 放到最前面，保证构建脚本在两种终端下都能正常执行。
$windowsPowerShellModules = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\Modules'
if (Test-Path -LiteralPath $windowsPowerShellModules) {
    $userWindowsPowerShellModules = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'WindowsPowerShell\Modules'
    $env:PSModulePath = @($userWindowsPowerShellModules, $windowsPowerShellModules, $env:PSModulePath) -join ';'
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$cargoBin = Join-Path $cargoHome 'bin'
$pathEntries = @($env:PATH -split ';' | Where-Object { $_ })
if (-not ($pathEntries | Where-Object { $_.TrimEnd('\') -ieq $cargoBin.TrimEnd('\') })) {
    $env:PATH = "$cargoBin;$env:PATH"
}
if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
    throw "cargo.exe 不在 PATH 中，请先安装 Rust：$cargoBin"
}
if (-not (Get-Command rustup.exe -ErrorAction SilentlyContinue)) {
    throw 'rustup.exe 不在 PATH 中，无法切换到 MSVC 工具链。'
}

$toolchain = 'stable-x86_64-pc-windows-msvc'
$installed = & rustup.exe toolchain list 2>$null
if ($LASTEXITCODE -ne 0 -or -not ($installed -match [regex]::Escape($toolchain))) {
    throw "缺少 Rust 工具链 '$toolchain'，请先运行：rustup toolchain install $toolchain"
}
$env:RUSTUP_TOOLCHAIN = $toolchain

$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
$installation = $null
if (Test-Path -LiteralPath $vswhere) {
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $installation) {
    $candidates = @(
        (Join-Path $programFilesX86 'Microsoft Visual Studio/2022/BuildTools'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio/2022/BuildTools'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio/2022/Community'),
        (Join-Path $programFilesX86 'Microsoft Visual Studio/2022/Community')
    ) | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'VC/Auxiliary/Build/vcvars64.bat') }
    if (@($candidates).Count -gt 0) { $installation = @($candidates)[0] }
}
if (-not $installation) {
    throw '未找到 Visual Studio C++ x64 生成工具（需要 VC.Tools.x86.x64）。'
}
$vcvars = Join-Path $installation 'VC/Auxiliary/Build/vcvars64.bat'
$lines = & cmd.exe /c ('call "' + $vcvars + '" >nul && set')
if ($LASTEXITCODE -ne 0) {
    throw '无法初始化 Visual Studio x64 构建环境。'
}
foreach ($line in $lines) {
    $separator = $line.IndexOf('=')
    if ($separator -gt 0) {
        [Environment]::SetEnvironmentVariable($line.Substring(0, $separator), $line.Substring($separator + 1), 'Process')
    }
}