<#
.SYNOPSIS
Fail unless a file carries a timestamped Authenticode signature.

.PARAMETER Path
The signed file.

.PARAMETER Policy
SignPath signing policy. Under release-signing the chain must be trusted by
Windows; test-signing uses a certificate Windows does not trust, so only the
presence of the signature and timestamp is checked.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Path,
    [string]$Policy = 'release-signing'
)

$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $Path
if (-not $signature.SignerCertificate) { throw "$Path is not signed." }
if (-not $signature.TimeStamperCertificate) { throw "$Path has no timestamp." }
if ($Policy -eq 'release-signing' -and $signature.Status -ne 'Valid') {
    throw "$Path signature status is $($signature.Status)."
}
Write-Host "$(Split-Path $Path -Leaf): signed by $($signature.SignerCertificate.Subject) ($($signature.Status))"
