param([string]$BuildDirectory = '.build-runtime-windows')
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$manifest = Get-Content -Raw (Join-Path $PSScriptRoot 'runtime-manifest.json') | ConvertFrom-Json
New-Item -ItemType Directory -Force -Path $BuildDirectory | Out-Null
$source = Join-Path $BuildDirectory 'source'
if (-not (Test-Path (Join-Path $source '.git'))) {
  git clone --no-checkout $manifest.repository $source
  if ($LASTEXITCODE -ne 0) { throw 'Runtime clone failed.' }
  git -C $source checkout --detach $manifest.revision
}
$revision = git -C $source rev-parse HEAD
if ($revision -ne $manifest.revision) { throw 'Runtime source revision differs from the pinned manifest.' }
$build = Join-Path $BuildDirectory 'build'
cmake -S $source -B $build -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TOOLS=ON -DLLAMA_BUILD_SERVER=ON -DLLAMA_OPENSSL=OFF -DGGML_NATIVE=OFF -DGGML_CUDA=OFF -DGGML_VULKAN=OFF -DBUILD_SHARED_LIBS=OFF -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
if ($LASTEXITCODE -ne 0) { throw 'Runtime configuration failed.' }
cmake --build $build --config Release --target llama-server --parallel 2
if ($LASTEXITCODE -ne 0) { throw 'Runtime compilation failed.' }
$bin = Join-Path $build 'bin/Release'
if (-not (Test-Path $bin)) { $bin = Join-Path $build 'bin' }
$target = Join-Path $project 'Resources/Runtime'
New-Item -ItemType Directory -Force -Path $target | Out-Null
$files = @{}
Get-ChildItem $bin -File | Where-Object { $_.Name -eq 'llama-server.exe' -or $_.Extension -eq '.dll' } | ForEach-Object {
  Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $target $_.Name) -Force
  # Windows PowerShell can inherit PowerShell 7's module search path in CI.
  # Hash through .NET so staging does not depend on Security module discovery.
  $stream = [System.IO.File]::OpenRead((Join-Path $target $_.Name))
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $files[$_.Name] = [System.BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha256.Dispose()
    $stream.Dispose()
  }
}
if (-not $files.ContainsKey('llama-server.exe')) { throw 'Built llama-server.exe missing.' }
$receipt = @{ revision = $manifest.revision; files = $files } | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText((Join-Path $target '.runtime-provenance.json'), $receipt, (New-Object System.Text.UTF8Encoding $false))
Write-Output 'Pinned runtime staged with exact checksums.'
