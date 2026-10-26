<#
.SYNOPSIS
    Smoke-test a Kavach Windows build.

.DESCRIPTION
    Automates the parts of the checklist in WINDOWS-BUILD.md section 4 that a script can do,
    then walks a human through the rest. A build is not a release until this has passed.

    Run on a Windows machine that is not your development box if possible.

.PARAMETER Installer
    Path to Kavach_*_x64-setup.exe (NSIS) or Kavach_*_x64_en-US.msi.

.PARAMETER SkipInstall
    Test an already-installed copy instead of installing.

.EXAMPLE
    .\scripts\smoke-test-windows.ps1 -Installer .\Kavach_0.5.0_x64-setup.exe
#>

[CmdletBinding()]
param(
    [string]$Installer,
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
$failures = 0
$checks = @()

function Record([string]$Name, [bool]$Ok, [string]$Detail = '') {
    $script:checks += [pscustomobject]@{ Check = $Name; Result = $(if ($Ok) { 'PASS' } else { 'FAIL' }); Detail = $Detail }
    if (-not $Ok) { $script:failures++ }
    $colour = if ($Ok) { 'Green' } else { 'Red' }
    Write-Host ("  {0}  {1} {2}" -f $(if ($Ok) { 'PASS' } else { 'FAIL' }), $Name, $Detail) -ForegroundColor $colour
}

function Ask([string]$Question) {
    $answer = Read-Host "  ? $Question [y/n]"
    return ($answer -match '^(y|yes)$')
}

Write-Host "Kavach Windows smoke test`n" -ForegroundColor Cyan

# ---------------------------------------------------------------- install
if (-not $SkipInstall) {
    if (-not $Installer) { throw "Pass -Installer <path>, or -SkipInstall to test an existing install." }
    if (-not (Test-Path $Installer)) { throw "Installer not found: $Installer" }

    $hash = (Get-FileHash -Algorithm SHA256 $Installer).Hash.ToLower()
    Write-Host "Installer: $Installer"
    Write-Host "SHA-256:   $hash"
    Write-Host "Compare this against BUILD-EVIDENCE.md before continuing.`n"

    if ($Installer -like '*.msi') {
        $log = Join-Path $env:TEMP 'kavach-msi.log'
        $process = Start-Process msiexec.exe -ArgumentList "/i `"$Installer`" /qn /l*v `"$log`"" -Wait -PassThru
    } else {
        $process = Start-Process $Installer -ArgumentList '/S' -Wait -PassThru
    }
    Record 'installer exits 0' ($process.ExitCode -eq 0) "exit $($process.ExitCode)"
}

# ---------------------------------------------------------------- locate
$candidates = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Kavach\kavach.exe'),
    (Join-Path $env:LOCALAPPDATA 'Kavach\kavach.exe'),
    (Join-Path $env:ProgramFiles 'Kavach\kavach.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Kavach\kavach.exe')
) | Where-Object { $_ -and (Test-Path $_) }

Record 'executable installed' ($candidates.Count -gt 0) ($candidates | Select-Object -First 1)
if ($candidates.Count -eq 0) {
    Write-Host "`nCannot continue without the executable." -ForegroundColor Red
    exit 1
}
$exe = $candidates[0]

$workspace = Join-Path $env:APPDATA 'app.kavach.builder'
if (Test-Path $workspace) { Remove-Item -Recurse -Force $workspace -ErrorAction SilentlyContinue }
Record 'workspace absent before first run' (-not (Test-Path $workspace))

# ---------------------------------------------------------------- launch
Write-Host "`nLaunching Kavach..." -ForegroundColor Cyan
$app = Start-Process $exe -PassThru
Start-Sleep -Seconds 6

$app.Refresh()
Record 'process is running' (-not $app.HasExited) "pid $($app.Id)"
Record 'main window has a title' ($app.MainWindowTitle -ne '') $app.MainWindowTitle
Record 'workspace created' (Test-Path $workspace) $workspace

# ---------------------------------------------------------------- manual
Write-Host "`nNow check the window. Each answer is a real check, so answer honestly.`n" -ForegroundColor Cyan
Record 'badge reads "desktop shell - local only"' (Ask 'Does the badge top-right say "desktop shell - local only"?')
Record 'device card shows real values' (Ask 'Does "This device" show CPU, cores, RAM and a non-zero disk free?')
Record 'disk free is non-zero' (Ask 'Is the disk figure a real number (not "not reported")?')
Record 'unmeasured list present' (Ask 'Does "What the probe could not measure" list at least two items?')
Record 'inference notice visible' (Ask 'Is the yellow notice saying local inference is not implemented visible?')
Record 'build completes with verify green' (Ask 'Did you press "Build it" and see 5 stages with verify green and 5 verified / 2 unverified?')
Record 'generated tests pass independently' (Ask 'Did you run "npm test" inside the generated project and see it pass?')
Record 'checkpoint restore works' (Ask 'Did Checkpoints -> Restore report files restored and removed?')

# ---------------------------------------------------------------- close
Write-Host "`nClosing the application..." -ForegroundColor Cyan
if (-not $app.HasExited) { $app.CloseMainWindow() | Out-Null; Start-Sleep -Seconds 3 }
if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force; Record 'closes cleanly' $false 'had to be killed' }
else { Record 'closes cleanly' $true }

$stragglers = Get-Process -Name 'kavach' -ErrorAction SilentlyContinue
Record 'no lingering processes' ($null -eq $stragglers)

# ---------------------------------------------------------------- uninstall
if (-not $SkipInstall) {
    $uninstaller = Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA 'Programs\Kavach'), (Join-Path $env:ProgramFiles 'Kavach') -Filter 'uninstall*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($uninstaller) {
        $result = Start-Process $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
        Start-Sleep -Seconds 3
        Record 'uninstaller exits 0' ($result.ExitCode -eq 0) "exit $($result.ExitCode)"
        Record 'executable removed' (-not (Test-Path $exe))
        Record 'workspace retained (documented behaviour)' (Test-Path $workspace)
    } else {
        Write-Host '  (no uninstaller found; uninstall manually)' -ForegroundColor Yellow
    }
}

# ---------------------------------------------------------------- summary
Write-Host "`n=== Summary ===" -ForegroundColor Cyan
$checks | Format-Table -AutoSize | Out-String | Write-Host

if ($failures -eq 0) {
    Write-Host "SMOKE TEST: PASS - $($checks.Count) checks" -ForegroundColor Green
    exit 0
}
Write-Host "SMOKE TEST: FAIL - $failures of $($checks.Count) checks failed" -ForegroundColor Red
exit 1
