<#
.SYNOPSIS
    Create and use a free self-signed code-signing certificate for Kavach.

.DESCRIPTION
    A code-signing certificate from a public CA costs money — an OV certificate is a few
    hundred dollars a year and an EV one more, plus a hardware token. This script creates a
    self-signed certificate instead, which is free and legitimate for internal distribution.

    Be clear about what it does and does not do:

      Does    - produces a signature that verifies, so the binary has not been tampered with
                after signing, and your own machines can trust it explicitly
      Does not - satisfy SmartScreen. Windows only trusts publicly-trusted CAs for
                reputation, so users will still see "Windows protected your PC" unless they
                install your certificate as a trusted publisher.

    For a public release you need a paid certificate. That is a business cost, not a
    technical one, and this script does not pretend otherwise.

.PARAMETER SignBundle
    Sign the installers that build-windows.ps1 just produced.

.PARAMETER Subject
    Certificate subject. Defaults to the publisher name used in tauri.conf.json.

.EXAMPLE
    .\scripts\make-self-signed-cert.ps1 -SignBundle
#>

[CmdletBinding()]
param(
    [switch]$SignBundle,
    [string]$Subject = 'CN=Kavach',
    [string]$Password = 'kavach-local-signing'
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

function Say([string]$Message) {
    Write-Host ""
    Write-Host $Message -ForegroundColor Cyan
}

Say "Locating signtool"
$signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue)?.Source
if (-not $signtool) {
    $candidates = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match 'x64' } | Select-Object -First 1
    if ($candidates) { $signtool = $candidates.FullName }
}
if (-not $signtool) {
    throw "signtool.exe not found. Install the Windows SDK (part of the C++ build tools workload)."
}
Write-Host "  $signtool"

Say "Creating a self-signed code-signing certificate"
$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $Subject `
    -KeyUsage DigitalSignature `
    -FriendlyName 'Kavach local code signing' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -NotAfter (Get-Date).AddYears(2) `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
Write-Host "  thumbprint: $($cert.Thumbprint)"

$pfx = Join-Path $env:TEMP 'kavach-selfsigned.pfx'
$secure = ConvertTo-SecureString -String $Password -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $secure | Out-Null
Write-Host "  exported:   $pfx"

Say "Trusting it on this machine only"
# This is what makes the signature useful locally. It changes nothing on anyone else's PC.
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPeople', 'CurrentUser')
$store.Open('ReadWrite')
$store.Add($cert)
$store.Close()
Write-Host "  added to CurrentUser\TrustedPeople"

if (-not $SignBundle) {
    Write-Host ""
    Write-Host "Certificate ready. Re-run with -SignBundle to sign the installers." -ForegroundColor Green
    Write-Host "Remember: this does not satisfy SmartScreen for other people."
    exit 0
}

Say "Signing the installers"
$bundle = 'apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle'
$files = Get-ChildItem -Path $bundle -Recurse -File -Include *.exe, *.msi -ErrorAction SilentlyContinue
if (-not $files) { throw "no installers found in $bundle — run scripts/build-windows.ps1 first" }

foreach ($file in $files) {
    & $signtool sign /fd sha256 /f $pfx /p $Password /tr http://timestamp.digicert.com /td sha256 $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "signing failed for $($file.Name)" }
    $result = & $signtool verify /pa $file.FullName
    Write-Host "  signed and verified: $($file.Name)"
}

Say "Summary"
foreach ($file in $files) {
    $hash = (Get-FileHash -Algorithm SHA256 $file.FullName).Hash.ToLower()
    $sig = Get-AuthenticodeSignature $file.FullName
    "{0}  {1}  status={2}" -f $hash, $file.Name, $sig.Status
}

Write-Host ""
Write-Host "Signed with a SELF-SIGNED certificate." -ForegroundColor Yellow
Write-Host "It verifies on this machine. It will not remove SmartScreen warnings for anyone else."
Write-Host "For a public release, buy an OV or EV code-signing certificate and use the"
Write-Host "signCommand configuration in WINDOWS-BUILD.md section 7."
