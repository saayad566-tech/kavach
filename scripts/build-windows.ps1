<#
.SYNOPSIS
    Build Kavach on Windows. No CI, no account, no cost.

.DESCRIPTION
    The 100% free path: everything runs on the machine in front of you and produces the same
    installers the CI job produces. Nothing here needs a paid service.

    Prerequisites (all free):
      - Visual Studio Build Tools 2022 with "Desktop development with C++"
      - Rust stable, MSVC toolchain  (https://rustup.rs)
      - Node.js 20+                  (https://nodejs.org)

.PARAMETER Test
    Run the full offline verification suite before building.

.PARAMETER SelfSign
    Also sign the installers with a locally generated self-signed certificate. This is free
    but does NOT stop SmartScreen warnings; see scripts/make-self-signed-cert.ps1.

.EXAMPLE
    .\scripts\build-windows.ps1 -Test
#>

[CmdletBinding()]
param(
    [switch]$Test,
    [switch]$SelfSign
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

function Say([string]$Message) {
    Write-Host ""
    Write-Host $Message -ForegroundColor Cyan
}

# ---------------------------------------------------------------- prerequisites
Say "1/5  Checking prerequisites"
python scripts/check-toolchain.py
if ($LASTEXITCODE -ne 0) { throw "a required tool is missing or too old (see above)" }
foreach ($tool in @('cargo', 'node')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool not found. Install Rust (https://rustup.rs) and Node 20+ (https://nodejs.org)."
    }
}
$rustTarget = (& rustc -vV) | Select-String '^host:'
Write-Host "  $rustTarget"
if ($rustTarget -notmatch 'windows-msvc') {
    throw "Rust is not using the MSVC toolchain. Run: rustup default stable-x86_64-pc-windows-msvc"
}

# ---------------------------------------------------------------- verification
Say "2/5  Offline verification"
if ($Test) {
    cargo test --offline --workspace
    if ($LASTEXITCODE -ne 0) { throw "tests failed" }
    python scripts/check-desktop-config.py --verbose
    python scripts/check-ui-contract.py
    python scripts/check-free.py
    node scripts/verify-ui.mjs
} else {
    cargo test --offline --workspace --quiet
    if ($LASTEXITCODE -ne 0) { throw "tests failed" }
}

# ---------------------------------------------------------------- build
Say "3/5  Building the application and installers"
Set-Location apps/desktop/src-tauri
cargo fetch
if ($LASTEXITCODE -ne 0) { throw "cargo fetch failed" }

npx --yes @tauri-apps/cli@2 build --target x86_64-pc-windows-msvc --bundles nsis,msi
if ($LASTEXITCODE -ne 0) { throw "the build failed" }

# ---------------------------------------------------------------- sign (optional, free)
if ($SelfSign) {
    Say "4/5  Signing with a self-signed certificate (free, does not satisfy SmartScreen)"
    & (Join-Path $PSScriptRoot 'make-self-signed-cert.ps1') -SignBundle
} else {
    Say "4/5  Skipping signing (pass -SelfSign to use a free self-signed certificate)"
}

# ---------------------------------------------------------------- artefacts
Say "5/5  Artefacts"
$bundle = 'target/x86_64-pc-windows-msvc/release/bundle'
$files = Get-ChildItem -Path $bundle -Recurse -File -Include *.exe, *.msi -ErrorAction SilentlyContinue
if (-not $files) { throw "no installer was produced in $bundle" }
$files | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLower()
    "{0}  {1}" -f $hash, $_.Name
}
$files | ForEach-Object { $_.FullName }

Write-Host ""
Write-Host "Done. Everything above ran locally and cost nothing." -ForegroundColor Green
Write-Host "No account, no certificate purchase, no hosted service was involved."
Write-Host ""
Write-Host "Next: run scripts/smoke-test-windows.ps1 -Installer <path to the .exe>"
