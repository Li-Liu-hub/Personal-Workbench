param(
    [Parameter(Mandatory=$true)][string]$Root,
    [ValidateSet('Speech','Compiler')][string]$Component = 'Speech',
    [string]$CacheDirectory
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Root = [IO.Path]::GetFullPath($Root)
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$stage = Join-Path $Root ('.setup-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
function Fetch([string]$Name,[string]$Url,[string]$Hash,[string]$Fallback='') {
    $file = Join-Path $stage $Name
    $cached = if ($CacheDirectory) { Join-Path $CacheDirectory $Name } else { '' }
    if ($cached -and (Test-Path -LiteralPath $cached) -and (Get-FileHash -LiteralPath $cached -Algorithm SHA256).Hash -eq $Hash) {
        Copy-Item -LiteralPath $cached -Destination $file
    } else {
        $urls = @($Url)
        if ($Fallback) { $urls += $Fallback }
        $ok = $false
        foreach ($address in $urls) {
            & curl.exe -L --fail --retry 1 --connect-timeout 15 --speed-limit 1024 --speed-time 25 --max-time 600 --silent --show-error -o $file $address
            if ($LASTEXITCODE -eq 0 -and (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -eq $Hash) { $ok = $true; break }
        }
        if (-not $ok) { throw "Download failed or SHA256 mismatch: $Name" }
    }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $Hash) { throw "SHA256 mismatch: $Name" }
    return $file
}
function Publish([string]$Source,[string]$Name) {
    $target = Join-Path $Root $Name
    if (Test-Path -LiteralPath $target) {
        $backup = Join-Path $Root ($Name + '.previous-' + [guid]::NewGuid().ToString('N'))
        Move-Item -LiteralPath $target -Destination $backup
        try { Move-Item -LiteralPath $Source -Destination $target }
        catch { Move-Item -LiteralPath $backup -Destination $target; throw }
    } else { Move-Item -LiteralPath $Source -Destination $target }
}
if ($Component -eq 'Compiler') {
    $archive = Fetch 'tcc.zip' 'https://download.savannah.gnu.org/releases/tinycc/tcc-0.9.27-win64-bin.zip' '34a721949a2583fdff725312da092fa0f5f1f284b702e6f811c6954714faabb2'
    Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $stage 'compiler')
    $tcc = Get-ChildItem (Join-Path $stage 'compiler') -Filter tcc.exe -Recurse -File | Select-Object -First 1
    if (-not $tcc) { throw 'Missing tcc.exe' }
    Publish $tcc.DirectoryName 'tcc'
    Write-Output 'TinyCC installed.'
} else {
    $archive = Fetch 'python.zip' 'https://www.python.org/ftp/python/3.11.9/python-3.11.9-embed-amd64.zip' '009d6bf7e3b2ddca3d784fa09f90fe54336d5b60f0e0f305c37f400bf83cfd3b'
    $pythonDir = Join-Path $stage 'python'
    Expand-Archive -LiteralPath $archive -DestinationPath $pythonDir
    [IO.File]::WriteAllText((Join-Path $pythonDir 'python311._pth'),"python311.zip`n.`nLib/site-packages`nimport site`n",[Text.UTF8Encoding]::new($false))
    $site = Join-Path $pythonDir 'Lib/site-packages'
    New-Item -ItemType Directory -Force -Path $site | Out-Null
    $numpy = Fetch 'numpy.zip' 'https://files.pythonhosted.org/packages/31/0a/f354fb7176b81747d870f7991dc763e157a934c717b67b58456bc63da3df/numpy-2.2.6-cp311-cp311-win_amd64.whl' 'e8213002e427c69c45a52bbd94163084025f533a55a59d6f9c5b820774ef3303'
    Expand-Archive -LiteralPath $numpy -DestinationPath $site -Force
    $sherpa = Fetch 'sherpa.zip' 'https://files.pythonhosted.org/packages/dd/1d/1f68369d1aeceffccdeeb0459ae56406cb1702d0efd7db7fba2df2778e37/sherpa_onnx-1.13.3-cp311-cp311-win_amd64.whl' '84cffcf1fc534d15fbcfe0274ca1c484b4f0bdc9a8a4615a9dd20b60fde0120a'
    Expand-Archive -LiteralPath $sherpa -DestinationPath $site -Force
    $core = Fetch 'sherpa-core.zip' 'https://files.pythonhosted.org/packages/0f/6e/d4c239c7e143aa18b4354490f14bd1737b61e46b04ff42d0648d2193de1c/sherpa_onnx_core-1.13.3-py3-none-win_amd64.whl' 'c0422955d525244599c7d1b57198ff5c9644cec021bf82f7a1067bfe44c34966'
    Expand-Archive -LiteralPath $core -DestinationPath $site -Force
    & (Join-Path $pythonDir 'python.exe') -c 'import sherpa_onnx, numpy; print(sherpa_onnx.__version__)'
    if ($LASTEXITCODE -ne 0) { throw 'Speech runtime import failed' }
    $model = Fetch 'kokoro.tar.bz2' 'https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2' 'a1e94694776049035c4f2c6529f003aaece993c76aae9a78995831c3c4dcafc6' 'https://hf-mirror.com/datasets/BowmanXavier/tts/resolve/main/kokoro-int8-multi-lang-v1_1.tar.bz2?download=true'
    & tar.exe -xf $model -C $stage
    if ($LASTEXITCODE -ne 0) { throw 'Model extraction failed' }
    $modelDir = Join-Path $stage 'kokoro-int8-multi-lang-v1_1'
    foreach ($name in @('model.int8.onnx','voices.bin','tokens.txt','lexicon-us-en.txt','lexicon-gb-en.txt','LICENSE')) {
        if (-not (Test-Path -LiteralPath (Join-Path $modelDir $name))) { throw "Missing model file: $name" }
    }
    Publish $pythonDir 'python'
    Publish $modelDir 'kokoro'
    Write-Output 'Kokoro offline speech installed.'
}
# Verify this exact staging directory before recursive cleanup.
$resolvedStage = [IO.Path]::GetFullPath($stage)
if ($resolvedStage.StartsWith($Root.TrimEnd('\') + '\',[StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedStage -Leaf).StartsWith('.setup-')) {
    Remove-Item -LiteralPath $resolvedStage -Recurse -Force
}
