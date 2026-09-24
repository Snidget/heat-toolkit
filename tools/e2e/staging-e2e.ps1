# Staging E2E harness for the HEAT3 licensing stack.
# Architecture plan section 17, scenario matrix 1-20.
#
# Requires a running Keygen CE staging instance and two built binaries:
#   - e2e_staging.exe          (src\bin\e2e_staging.rs)
#   - heat3-license-admin.exe  (tools\license-admin)
#   - heat3_povorotnik.exe     (optional, for the secret-scan scenario 18)
#
# Environment (staging values, same account across admin and client):
#   HEAT3_E2E_API_URL HEAT3_E2E_ACCOUNT_ID HEAT3_E2E_PRODUCT_ID
#   HEAT3_E2E_POLICY_ID HEAT3_E2E_PUBLIC_KEY HEAT3_E2E_OFFLINE_TTL (optional)
#   HEAT3_ADMIN_API_URL HEAT3_ADMIN_ACCOUNT_ID HEAT3_ADMIN_POLICY_ID
#   HEAT3_KEYGEN_ADMIN_TOKEN HEAT3_ADMIN_AUDIT_KEY
#
# Usage:
#   .\staging-e2e.ps1 -ProbeExe target\release\e2e_staging.exe `
#       -AdminExe tools\license-admin\target\release\heat3-license-admin.exe `
#       [-ClientExe target\release\heat3_povorotnik.exe]
# Exit code 0 = all automated scenarios passed.

param(
    [Parameter(Mandatory = $true)][string]$ProbeExe,
    [Parameter(Mandatory = $true)][string]$AdminExe,
    [string]$ClientExe = "",
    [string]$LicenseNamePrefix = "e2e-",
    [string]$ReportPath = ""
)

$ErrorActionPreference = "Stop"

$probePath = (Resolve-Path -LiteralPath $ProbeExe -ErrorAction Stop).Path
$adminPath = (Resolve-Path -LiteralPath $AdminExe -ErrorAction Stop).Path
if (-not $ReportPath) {
    $reportDir = Join-Path $PSScriptRoot "..\..\docs\reviews"
    $ReportPath = Join-Path $reportDir ("staging-e2e-report-" + (Get-Date -Format "yyyy-MM-dd") + ".md")
}

$probeEnv = @(
    "HEAT3_E2E_API_URL", "HEAT3_E2E_ACCOUNT_ID", "HEAT3_E2E_PRODUCT_ID",
    "HEAT3_E2E_POLICY_ID", "HEAT3_E2E_PUBLIC_KEY"
)
$adminEnv = @(
    "HEAT3_ADMIN_API_URL", "HEAT3_ADMIN_ACCOUNT_ID", "HEAT3_ADMIN_POLICY_ID",
    "HEAT3_KEYGEN_ADMIN_TOKEN", "HEAT3_ADMIN_AUDIT_KEY"
)

function Assert-Env {
    param([string[]]$Names)
    foreach ($name in $Names) {
        if (-not [Environment]::GetEnvironmentVariable($name)) {
            throw "Missing environment variable $name"
        }
    }
}

function Invoke-Probe {
    param([string[]]$Arguments)
    $output = & $probePath @Arguments 2>&1
    $code = $LASTEXITCODE
    foreach ($line in $output) { Write-Host "  [probe] $line" }
    $text = ($output | Out-String)
    [PSCustomObject]@{ Code = $code; Text = $text }
}

function Invoke-Admin {
    param([string[]]$Arguments)
    $output = & $adminPath @Arguments 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "admin CLI failed ($($LASTEXITCODE)): $output"
    }
    $output
}

function Add-Result {
    # IDs are strings so automated sub-checks can use unique identifiers
    # (e.g. "3b") instead of emitting two rows with the same scenario ID.
    param([string]$Id, [string]$Name, [string]$Status, [string]$Note)
    $script:results.Add([PSCustomObject]@{
        Id = $Id; Name = $Name; Status = $Status; Note = $Note
    })
}

function Write-Report {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("# Staging E2E report - HEAT3 Povorotnik licensing")
    $lines.Add("")
    $lines.Add("- Date: $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
    $lines.Add("- API: $env:HEAT3_E2E_API_URL")
    $lines.Add("- Probe: $probePath")
    $lines.Add("- Admin: $adminPath")
    $lines.Add("")
    $passed = @($script:results | Where-Object { $_.Status -eq "PASS" }).Count
    $failed = @($script:results | Where-Object { $_.Status -eq "FAIL" }).Count
    $manual = @($script:results | Where-Object { $_.Status -eq "MANUAL" }).Count
    $duplicates = @($script:results | Group-Object Id | Where-Object { $_.Count -gt 1 })
    if ($duplicates) {
        $ids = ($duplicates | ForEach-Object { $_.Name }) -join ", "
        throw "Duplicate scenario IDs in one run: $ids"
    }
    $lines.Add("## Summary: PASS=$passed FAIL=$failed MANUAL=$manual")
    $lines.Add("")
    $lines.Add("| # | Scenario | Status | Notes |")
    $lines.Add("|---:|---|---|---|")
    foreach ($row in $script:results) {
        $note = $row.Note -replace "\|", "/"
        $lines.Add("| $($row.Id) | $($row.Name) | $($row.Status) | $note |")
    }
    $lines.Add("")
    $lines.Add("Automated checks exit with 0 on success and 2 on an expected failure;")
    $lines.Add("the harness treats exit 2 as PASS when the scenario demands failure.")
    $dir = Split-Path -Parent $ReportPath
    if ($dir) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    Set-Content -LiteralPath $ReportPath -Value $lines -Encoding UTF8
    Write-Host "Report written to $ReportPath"
}

$results = New-Object System.Collections.Generic.List[object]
$licenseKey = ""
$licenseId = ""
$machineId = ""
$certPath = ""
$tamperedPath = ""
$suspended = $false
$revoked = $false

try {
    Assert-Env $probeEnv
    Assert-Env $adminEnv
    if (-not $env:HEAT3_ADMIN_AUDIT_PATH) {
        $env:HEAT3_ADMIN_AUDIT_PATH = Join-Path $env:TEMP "heat3-e2e-audit.jsonl"
    }
    if (-not $env:HEAT3_E2E_OFFLINE_TTL) {
        $env:HEAT3_E2E_OFFLINE_TTL = "604800"
    }

    Write-Host "== Scenario 1: issue a valid license and activate =="
    $name = $LicenseNamePrefix + (Get-Date -Format "yyyyMMddHHmmss")
    $issued = Invoke-Admin @("issue", "--name", $name)
    $lines = @($issued -split "`r?`n" | Where-Object { $_.Trim() -ne "" })
    $idLine = $lines | Where-Object { $_ -match '^ID: ([A-Za-z0-9_-]+)$' }
    if (-not $idLine) {
        throw "cannot parse license ID from admin output: $issued"
    }
    $licenseId = $idLine -replace '^ID:\s*', ''
    $keyLine = $lines[-1]
    $licenseKey = $keyLine -replace '^[^:]+:\s*', ''
    if ($licenseKey.Length -lt 8) {
        throw "cannot parse license key from admin output: $issued"
    }
    Write-Host "license_id=$licenseId"

    # A strict node-locked policy answers the first validation of a legitimate,
    # not-yet-activated license with NO_MACHINE; the activate probe below must
    # handle that documented pre-activation state.
    $r1 = Invoke-Probe @("validate", $licenseKey)
    if ($r1.Code -eq 0) {
        Add-Result 1 "Valid new key, network available: one machine created, functions open" "PASS" "validate exit 0 (already valid)"
    } elseif ($r1.Code -eq 2 -and $r1.Text -match 'code=NoMachine') {
        Add-Result 1 "Valid new key, network available: one machine created, functions open" "PASS" "strict node-locked pre-activation state NoMachine observed"
    } else {
        Add-Result 1 "Valid new key, network available" "FAIL" "validate exit $($r1.Code) output: $($r1.Text)"
    }

    $certPath = Join-Path $env:TEMP ("heat3-e2e-" + [guid]::NewGuid().ToString("N") + ".cert")
    $r2a = Invoke-Probe @("activate", $licenseKey, $certPath)
    $r2b = $null
    if ($r2a.Code -eq 0 -and $r2a.Text -match 'machine_id=(\S+)') {
        $machineId = $Matches[1]
        # Rerun on the same PC: find-before-create must return the same machine.
        $r2b = Invoke-Probe @("activate", $licenseKey, $certPath)
        if ($r2b.Code -eq 0 -and $r2b.Text -match 'machine_id=(\S+)' -and $Matches[1] -eq $machineId) {
            Add-Result 2 "Rerun on the same PC: duplicate machine is not created" "PASS" "find-before-create idempotent, machine_id=$machineId"
        } else {
            Add-Result 2 "Rerun on the same PC: duplicate machine is not created" "FAIL" "second activate exit $($r2b.Code) output: $($r2b.Text)"
        }
    } else {
        Add-Result 2 "Rerun on the same PC: duplicate machine is not created" "FAIL" "activate exit $($r2a.Code) output: $($r2a.Text)"
    }

    $r3 = Invoke-Probe @("checkin", $licenseKey, $licenseId)
    if ($r3.Code -eq 0) {
        Add-Result 3 "No network for a day, TTL still valid (check-in path)" "PASS" "checkin ok"
    } else {
        Add-Result 3 "No network for a day, TTL still valid" "FAIL" "checkin exit $($r3.Code)"
    }
    Add-Result 4 "No network after TTL: NeedsOnline, functions closed" "MANUAL" "GUI: unplug network > TTL, relaunch app"

    $r5 = Invoke-Probe @("verify-offline", $licenseKey, $licenseId, $machineId, $certPath)
    if ($r5.Code -eq 0) {
        Add-Result 11 "Valid offline certificate verifies" "PASS" "offline verify of live machine file ok"
    } else {
        Add-Result 11 "Valid offline certificate verifies" "FAIL" "offline verify exit $($r5.Code)"
    }

    $tamperedPath = $certPath + ".tampered"
    $bytes = [IO.File]::ReadAllBytes($certPath)
    $mid = [int][Math]::Floor($bytes.Length / 2)
    $bytes[$mid] = $bytes[$mid] -bxor 0x01
    [IO.File]::WriteAllBytes($tamperedPath, $bytes)
    $r6 = Invoke-Probe @("verify-offline", $licenseKey, $licenseId, $machineId, $tamperedPath)
    if ($r6.Code -eq 2) {
        Add-Result 15 "Corrupted record is not accepted" "PASS" "tampered certificate rejected (exit 2)"
    } else {
        Add-Result 15 "Corrupted record is not accepted" "FAIL" "tamper verify exit $($r6.Code), expected 2"
    }

    $r7 = Invoke-Probe @("checkout", $licenseKey, $licenseId, $machineId)
    if ($r7.Code -ne 0) {
        throw "baseline checkout failed before suspend (exit $($r7.Code)); staging may be broken"
    }

    Write-Host "== Suspend and verify access is closed =="
    $null = Invoke-Admin @("suspend", $licenseId)
    $suspended = $true
    $r8 = Invoke-Probe @("checkout", $licenseKey, $licenseId, $machineId)
    $r9 = Invoke-Probe @("validate", $licenseKey)
    if ($r8.Code -eq 2 -and $r9.Code -eq 2) {
        Add-Result 5 "Suspend during online session closes access" "PASS" "checkout and validate both rejected while suspended"
    } else {
        Add-Result 5 "Suspend during online session closes access" "FAIL" "checkout exit $($r8.Code), validate exit $($r9.Code), expected 2"
    }
    Add-Result 6 "Suspend with fully offline client: access until old TTL" "MANUAL" "GUI: disconnect network, keep running past TTL, expect close"

    Write-Host "== Reinstate and verify access is restored =="
    $null = Invoke-Admin @("reinstate", $licenseId)
    $suspended = $false
    $r10 = Invoke-Probe @("checkout", $licenseKey, $licenseId, $machineId)
    $r11 = Invoke-Probe @("checkin", $licenseKey, $licenseId)
    if ($r10.Code -eq 0 -and $r11.Code -eq 0) {
        Add-Result 7 "Reinstate: access restored after online refresh" "PASS" "checkout and checkin ok after reinstate"
    } else {
        Add-Result 7 "Reinstate: access restored" "FAIL" "checkout exit $($r10.Code), checkin exit $($r11.Code)"
    }
    Add-Result 8 "Envelope copied to another Windows user/PC fails" "MANUAL" "GUI: copy app data to a second Windows user, expect DPAPI failure"
    Add-Result 9 "One allowed component changed: majority match" "MANUAL" "VM: change volume label/system serial, rerun activation UI"
    Add-Result 10 "Move to a substantially different PC: mismatch" "MANUAL" "VM: different hardware profile, expect rejection"
    Add-Result 12 "Clock rollback does not extend TTL" "MANUAL" "GUI: set clock back past TTL, expect NeedsOnline"

    Write-Host "== Network failure error mapping (dead endpoint) =="
    $previousApiUrl = $env:HEAT3_E2E_API_URL
    try {
        $env:HEAT3_E2E_API_URL = "https://127.0.0.1:9/"
        $r12 = Invoke-Probe @("expect-network-failure", $licenseKey, $licenseId, $machineId)
        if ($r12.Code -eq 0) {
            # The probe classifies the transport failure only; the lease
            # retention state machine is a distinct MANUAL scenario below.
            Add-Result 13 "Keygen 5xx/timeout classified as transient outage" "PASS" "transport failure mapped to Timeout/Connection/Transport/5xx"
        } else {
            Add-Result 13 "Keygen 5xx/timeout classified as transient outage" "FAIL" "expect-network-failure exit $($r12.Code)"
        }
        Add-Result "13b" "Keygen 5xx/timeout with valid lease: old TTL kept" "MANUAL" "requires manager/state-machine run: pre-existing signed lease, outage, access kept until the old expiry"
        Add-Result 14 "Keygen 5xx/timeout without lease: fail closed" "MANUAL" "requires a genuine no-cached-lease UI/state-machine run"
    }
    finally {
        $env:HEAT3_E2E_API_URL = $previousApiUrl
    }

    Write-Host "== Deactivate and verify the machine is removed =="
    $r13 = Invoke-Probe @("deactivate", $licenseKey, $licenseId, $machineId)
    if ($r13.Code -eq 0) {
        Add-Result 16 "Online deactivation removes the machine" "PASS" "machine deleted, find_machine empty"
    } else {
        Add-Result 16 "Online deactivation removes the machine" "FAIL" "deactivate exit $($r13.Code)"
    }

    if ($ClientExe) {
        Write-Host "== Scenario 18: scan release artifact for secrets =="
        $clientPath = (Resolve-Path -LiteralPath $ClientExe -ErrorAction Stop).Path
        $clientBytes = [IO.File]::ReadAllBytes($clientPath)
        $clientText = [Text.Encoding]::ASCII.GetString($clientBytes)
        $leaks = @()
        if ($clientText.Contains($env:HEAT3_KEYGEN_ADMIN_TOKEN)) { $leaks += "admin token" }
        if ($clientText.Contains($licenseKey)) { $leaks += "issued license key" }
        if ($leaks.Count -eq 0) {
            Add-Result 18 "Secrets absent from release client binary" "PASS" "no admin token / license key in client binary"
        } else {
            Add-Result 18 "Secrets absent from release client binary" "FAIL" ("leaked: " + ($leaks -join ", "))
        }
    } else {
        Add-Result 18 "Secrets absent from release client binary" "MANUAL" "pass -ClientExe to scan the binary"
    }

    Write-Host "== Revoke and verify the key dies =="
    $null = Invoke-Admin @("revoke", $licenseId, "--confirm", $licenseId)
    $revoked = $true
    $r14 = Invoke-Probe @("validate", $licenseKey)
    if ($r14.Code -eq 2) {
        Add-Result 17 "Revoke via CLI: license cannot recover" "PASS" "validation rejected after revoke"
    } else {
        Add-Result 17 "Revoke via CLI: license cannot recover" "FAIL" "validate exit $($r14.Code), expected 2"
    }

    Add-Result 19 "Production backup restored on staging validates" "MANUAL" "docker volume backup/restore drill on the Keygen CE instance"
    Add-Result 20 "Window move/resize during refresh: no freezes" "MANUAL" "GUI: drag window while license refresh runs"
    Add-Result "3b" "No network for a day, TTL still valid (offline UI part)" "MANUAL" "GUI: run offline before TTL expires"
}
finally {
    if ($suspended) {
        try { $null = Invoke-Admin @("reinstate", $licenseId) } catch { Write-Warning "cleanup reinstate failed: $_" }
    }
    if (-not $revoked -and $licenseId) {
        try { $null = Invoke-Admin @("revoke", $licenseId, "--confirm", $licenseId) } catch { Write-Warning "cleanup revoke failed: $_" }
    }
    foreach ($path in @($certPath, $tamperedPath)) {
        if ($path) { Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue }
    }
    Write-Report
}

$failed = @($results | Where-Object { $_.Status -eq "FAIL" }).Count
if ($failed -gt 0) {
    Write-Error "E2E finished with $failed failed scenario(s)"
    exit 1
}
Write-Host "E2E finished: all automated scenarios passed."
exit 0
