param([Parameter(Mandatory=$true)][string]$Artifact, [Parameter(Mandatory=$true)][string]$Thumbprint)
$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $Artifact
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint) {
  throw 'Official Authenticode signature is missing, invalid, or belongs to a different signer.'
}
Write-Output 'Authenticode signature and expected signer verified. Installation acceptance is a separate gate.'
