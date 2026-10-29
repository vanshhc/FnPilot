$ErrorActionPreference = 'Stop'
$thumbprint = $env:WINDOWS_CERTIFICATE_THUMBPRINT
if (-not $thumbprint -or -not $env:WINDOWS_TIMESTAMP_URL.StartsWith('https://')) { throw 'Expected signing identity and HTTPS timestamp service are required.' }
$pfx = Join-Path $env:RUNNER_TEMP 'fnpilot-release.pfx'
[System.IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_PFX_BASE64))
$certificate = $null
try {
  $password = ConvertTo-SecureString $env:WINDOWS_PFX_PASSWORD -AsPlainText -Force
  $certificate = Import-PfxCertificate -FilePath $pfx -CertStoreLocation Cert:\CurrentUser\My -Password $password
  if ($certificate.Thumbprint -ne $thumbprint) { throw 'Imported signer differs from protected expected identity.' }
  $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $signtool) { throw 'Windows SDK signing tool is unavailable.' }
  Get-ChildItem apps/desktop/src-tauri/resources -Recurse -File | Where-Object { $_.Extension -in '.exe','.dll' } | ForEach-Object {
    & $signtool.FullName sign /sha1 $thumbprint /fd SHA256 /tr $env:WINDOWS_TIMESTAMP_URL /td SHA256 $_.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Embedded code signing failed.' }
    & ./scripts/release/verify-windows.ps1 -Artifact $_.FullName -Thumbprint $thumbprint
  }
  $provenance = 'apps/desktop/src-tauri/resources/runtime/provenance.json'
  $receipt = Get-Content -Raw $provenance | ConvertFrom-Json
  foreach ($property in $receipt.files.PSObject.Properties) {
    $property.Value = (Get-FileHash -LiteralPath (Join-Path 'apps/desktop/src-tauri/resources/runtime' $property.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
  }
  [System.IO.File]::WriteAllText((Join-Path (Get-Location) $provenance), ($receipt | ConvertTo-Json -Depth 5), (New-Object System.Text.UTF8Encoding $false))
  $config = @{ bundle = @{ windows = @{ certificateThumbprint = $thumbprint; digestAlgorithm = 'sha256'; timestampUrl = $env:WINDOWS_TIMESTAMP_URL; webviewInstallMode = @{ type = 'offlineInstaller'; silent = $true } } } } | ConvertTo-Json -Depth 5
  $configPath = Join-Path $env:RUNNER_TEMP 'fnpilot-signing.json'
  [System.IO.File]::WriteAllText($configPath, $config, (New-Object System.Text.UTF8Encoding $false))
  npm run tauri --workspace '@fnpilot/desktop' -- build --config src-tauri/tauri.official.conf.json --config $configPath
  if ($LASTEXITCODE -ne 0) { throw 'Signed Tauri build failed.' }
  & ./scripts/release/verify-windows.ps1 -Artifact target/release/fnpilot-desktop.exe -Thumbprint $thumbprint
  New-Item -ItemType Directory -Force target/distribution | Out-Null
  $installers = Get-ChildItem target/release/bundle/nsis -Filter '*.exe'
  if ($installers.Count -ne 1) { throw 'Expected exactly one release installer.' }
  foreach ($installer in $installers) {
    & ./scripts/release/verify-windows.ps1 -Artifact $installer.FullName -Thumbprint $thumbprint
    Copy-Item $installer.FullName target/distribution/
  }
} finally {
  Remove-Item -LiteralPath $pfx -Force -ErrorAction SilentlyContinue
  if ($certificate) { Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -DeleteKey -ErrorAction SilentlyContinue }
}
