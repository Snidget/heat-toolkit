# Signs the release binary with an Authenticode certificate, packages it as a
# ZIP and builds an MSI installer with the WiX toolset (architecture plan
# section 8.1, W8).
#
# Without HEAT3_CODESIGN_CERT_BASE64 the script skips signing (and exits 0 with
# -SkipIfNoCert) so local runs and CI without a certificate stay green. Without
# the WiX toolset the MSI step is skipped with a warning - unless -RequireMsi,
# which makes a trusted release fail closed instead of silently shipping
# without its installer.
#
# -SmokeTest performs a credential-free installer lifecycle smoke: it builds
# two dummy MSI versions, installs v1, major-upgrades to v2, and uninstalls v2
# without touching signing secrets or the production UpgradeCode. Intended for
# PR CI and uses a unique package identity on each run.
#
# Usage (from repo root):
#   .\tools\release\sign-and-package.ps1 [-SkipIfNoCert] [-WixRoot C:\...\WiX Toolset v3.14]
#   .\tools\release\sign-and-package.ps1 -SmokeTest
# Env: HEAT3_CODESIGN_CERT_BASE64, HEAT3_CODESIGN_CERT_PASSWORD
param(
    [string]$BinaryPath = "target\release\heat3_povorotnik.exe",
    [string]$OutDir = "dist",
    [string]$Version = "",
    [string]$CertBase64 = $env:HEAT3_CODESIGN_CERT_BASE64,
    [string]$CertPassword = $env:HEAT3_CODESIGN_CERT_PASSWORD,
    [string]$TimestampServer = "http://timestamp.digicert.com",
    [string]$WixRoot = $env:WIX,
    [switch]$SkipIfNoCert,
    [switch]$RequireMsi,
    [switch]$SmokeTest
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$wxsPath = Join-Path $scriptDir "heat3_povorotnik.wxs"

if (-not $Version) {
    $match = Select-String -LiteralPath "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1
    if ($match) {
        $Version = $match.Matches[0].Groups[1].Value
    } else {
        $Version = "0.0.0-local"
    }
}

function Resolve-WixToolset {
    foreach ($root in @($WixRoot, "C:\Program Files (x86)\WiX Toolset v3.14", "C:\Program Files\WiX Toolset v3.14")) {
        if (-not $root) { continue }
        $c = Join-Path $root "bin\candle.exe"
        $l = Join-Path $root "bin\light.exe"
        if ((Test-Path -LiteralPath $c) -and (Test-Path -LiteralPath $l)) {
            return @{ Candle = $c; Light = $l }
        }
    }
    return $null
}

function Build-Msi {
    param(
        [Parameter(Mandatory = $true)][string]$Binary,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$PackageVersion,
        [string]$ProductName = "HEAT3 Povorotnik",
        [string]$InstallDirectoryName = "HEAT3 Povorotnik",
        [string]$UpgradeCode = "D5A2E3B1-7C84-4F2A-9B61-3E0D8C4A1F57"
    )
    $wix = Resolve-WixToolset
    if (-not $wix) {
        throw "WiX toolset not found; MSI packaging is required here (installer hook W8)."
    }
    $wixOut = Join-Path $env:TEMP ("heat3-wix-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $wixOut | Out-Null
    try {
        & $wix.Candle -arch x64 "-dHeat3Exe=$Binary" "-dHeat3Version=$PackageVersion" `
            "-dHeat3ProductName=$ProductName" "-dHeat3InstallDirectoryName=$InstallDirectoryName" `
            "-dHeat3UpgradeCode=$UpgradeCode" -out "$wixOut\" $wxsPath
        if ($LASTEXITCODE -ne 0) {
            throw "candle failed with exit code $LASTEXITCODE"
        }
        & $wix.Light -out $Destination (Join-Path $wixOut "heat3_povorotnik.wixobj")
        if ($LASTEXITCODE -ne 0) {
            throw "light failed with exit code $LASTEXITCODE"
        }
        if (-not (Test-Path -LiteralPath $Destination)) {
            throw "MSI was not produced at $Destination"
        }
    }
    finally {
        Remove-Item -LiteralPath $wixOut -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-MsiSmokeStep {
    param(
        [Parameter(Mandatory = $true)][string[]]$MsiArguments,
        [Parameter(Mandatory = $true)][string]$StepName,
        [Parameter(Mandatory = $true)][string]$LogDirectory
    )

    $logPath = Join-Path $LogDirectory "$StepName.log"
    $arguments = @($MsiArguments) + @("/qn", "/norestart", "/L*v", $logPath)
    $quotedArguments = $arguments | ForEach-Object { '"' + $_.Replace('"', '\"') + '"' }
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = "$env:SystemRoot\System32\msiexec.exe"
    $startInfo.Arguments = $quotedArguments -join ' '
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $process = [System.Diagnostics.Process]::Start($startInfo)
    try {
        if (-not $process.WaitForExit(180000)) {
            $process.Kill()
            $process.WaitForExit()
            throw "MSI smoke step '$StepName' exceeded the 180-second timeout."
        }
        $exitCode = $process.ExitCode
    } finally {
        $process.Dispose()
    }
    if ($exitCode -notin @(0, 3010)) {
        $logTail = if (Test-Path -LiteralPath $logPath) {
            (Get-Content -LiteralPath $logPath -Tail 50) -join [Environment]::NewLine
        } else {
            "No Windows Installer log was produced."
        }
        throw "MSI smoke step '$StepName' failed with exit code $exitCode.`n$logTail"
    }
    Write-Output "MSI smoke step '$StepName' completed (exit code $exitCode)."
}

function Get-RelatedProductCodes {
    param(
        [Parameter(Mandatory = $true)][object]$Installer,
        [Parameter(Mandatory = $true)][string]$UpgradeCode
    )

    $productCodes = [System.Collections.Generic.List[string]]::new()
    foreach ($productCode in $Installer.RelatedProducts($UpgradeCode)) {
        [void]$productCodes.Add([string]$productCode)
    }
    return $productCodes.ToArray()
}

function Assert-SmokePayload {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][byte[]]$ExpectedBytes,
        [Parameter(Mandatory = $true)][string]$StepName,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        $logTail = if (Test-Path -LiteralPath $LogPath) {
            (Get-Content -LiteralPath $LogPath -Tail 100) -join [Environment]::NewLine
        } else {
            "No Windows Installer log was produced at $LogPath."
        }
        throw "MSI smoke step '$StepName' did not install $Path.`nMSI log tail:`n$logTail"
    }
    $actualBytes = [IO.File]::ReadAllBytes($Path)
    if ([Convert]::ToBase64String($actualBytes) -ne [Convert]::ToBase64String($ExpectedBytes)) {
        throw "MSI smoke step '$StepName' did not install the expected payload."
    }
}

function Wait-SmokePayload {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][byte[]]$ExpectedBytes,
        [Parameter(Mandatory = $true)][string]$StepName,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            Assert-SmokePayload -Path $Path -ExpectedBytes $ExpectedBytes `
                -StepName $StepName -LogPath $LogPath
            return
        }
        Start-Sleep -Milliseconds 500
    }
    Assert-SmokePayload -Path $Path -ExpectedBytes $ExpectedBytes `
        -StepName $StepName -LogPath $LogPath
}

if ($SmokeTest) {
    # Build and exercise a complete MSI lifecycle without signing credentials.
    if (-not (Resolve-WixToolset)) {
        throw "WiX toolset not found; the requested MSI lifecycle smoke cannot run."
    }
    $smokeDir = Join-Path $env:TEMP ("heat3-package-smoke-" + [guid]::NewGuid().ToString("N"))
    $smokeId = [guid]::NewGuid().ToString("N")
    $smokeUpgradeCode = ([guid]::NewGuid()).ToString("B").ToUpperInvariant()
    $smokeProductName = "HEAT3 Povorotnik Smoke $smokeId"
    $smokeInstallDirectoryName = "HEAT3 Povorotnik Smoke $smokeId"
    if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
        throw "MSI lifecycle smoke requires a three-part numeric ProductVersion; got '$Version'."
    }
    $major = [int]$Matches[1]
    $minor = [int]$Matches[2]
    $patch = [int]$Matches[3]
    if ($patch -lt 255) {
        $patch++
    } elseif ($minor -lt 255) {
        $minor++
        $patch = 0
    } elseif ($major -lt 255) {
        $major++
        $minor = 0
        $patch = 0
    } else {
        throw "MSI lifecycle smoke cannot create a version newer than '$Version'."
    }
    $upgradeVersion = "$major.$minor.$patch"
    $programFiles = [Environment]::GetFolderPath("ProgramFiles")
    if (-not $programFiles) {
        throw "Could not resolve the 64-bit Program Files directory for MSI lifecycle smoke."
    }
    $smokeInstallDirectory = Join-Path $programFiles $smokeInstallDirectoryName
    $payloadV1 = [byte[]](0x4D, 0x5A, 0x31, 0x00)
    $payloadV2 = [byte[]](0x4D, 0x5A, 0x32, 0x00)
    $installer = $null
    try {
        New-Item -ItemType Directory -Force -Path $smokeDir | Out-Null
        $dummyV1 = Join-Path $smokeDir "heat3_povorotnik-v1.exe"
        $dummyV2 = Join-Path $smokeDir "heat3_povorotnik-v2.exe"
        [IO.File]::WriteAllBytes($dummyV1, $payloadV1)
        [IO.File]::WriteAllBytes($dummyV2, $payloadV2)

        $msiV1 = Join-Path $smokeDir "heat3-povorotnik-smoke-v1.msi"
        $msiV2 = Join-Path $smokeDir "heat3-povorotnik-smoke-v2.msi"
        Build-Msi -Binary $dummyV1 -Destination $msiV1 -PackageVersion $Version `
            -ProductName $smokeProductName -InstallDirectoryName $smokeInstallDirectoryName `
            -UpgradeCode $smokeUpgradeCode
        Build-Msi -Binary $dummyV2 -Destination $msiV2 -PackageVersion $upgradeVersion `
            -ProductName $smokeProductName -InstallDirectoryName $smokeInstallDirectoryName `
            -UpgradeCode $smokeUpgradeCode

        $installer = New-Object -ComObject WindowsInstaller.Installer
        Invoke-MsiSmokeStep -MsiArguments @("/i", $msiV1) -StepName "install-v1" -LogDirectory $smokeDir
        Wait-SmokePayload -Path (Join-Path $smokeInstallDirectory "heat3_povorotnik.exe") `
            -ExpectedBytes $payloadV1 -StepName "install-v1" -LogPath (Join-Path $smokeDir "install-v1.log")
        $v1ProductCodes = @(Get-RelatedProductCodes -Installer $installer -UpgradeCode $smokeUpgradeCode)
        if ($v1ProductCodes.Count -ne 1) {
            throw "MSI lifecycle smoke expected one installed v1 product, found $($v1ProductCodes.Count)."
        }
        $v1ProductCode = $v1ProductCodes[0]
        $v1Version = [string]$installer.ProductInfo($v1ProductCode, "VersionString")
        if ($v1Version -ne $Version) {
            throw "MSI lifecycle smoke installed version '$v1Version', expected '$Version'."
        }

        Invoke-MsiSmokeStep -MsiArguments @("/i", $msiV2) -StepName "upgrade-v2" -LogDirectory $smokeDir
        Wait-SmokePayload -Path (Join-Path $smokeInstallDirectory "heat3_povorotnik.exe") `
            -ExpectedBytes $payloadV2 -StepName "upgrade-v2" -LogPath (Join-Path $smokeDir "upgrade-v2.log")
        $v2ProductCodes = @(Get-RelatedProductCodes -Installer $installer -UpgradeCode $smokeUpgradeCode)
        if ($v2ProductCodes.Count -ne 1) {
            throw "MSI major upgrade left $($v2ProductCodes.Count) related products installed; expected exactly one."
        }
        $v2ProductCode = $v2ProductCodes[0]
        if ($v2ProductCode -eq $v1ProductCode) {
            throw "MSI major upgrade did not replace the v1 ProductCode."
        }
        $v2Version = [string]$installer.ProductInfo($v2ProductCode, "VersionString")
        if ($v2Version -ne $upgradeVersion) {
            throw "MSI major upgrade installed version '$v2Version', expected '$upgradeVersion'."
        }

        Invoke-MsiSmokeStep -MsiArguments @("/x", $v2ProductCode) -StepName "uninstall-v2" -LogDirectory $smokeDir
        $remainingProductCodes = @(Get-RelatedProductCodes -Installer $installer -UpgradeCode $smokeUpgradeCode)
        if ($remainingProductCodes.Count -ne 0) {
            throw "MSI uninstall left $($remainingProductCodes.Count) related products installed."
        }
        if (Test-Path -LiteralPath (Join-Path $smokeInstallDirectory "heat3_povorotnik.exe")) {
            throw "MSI uninstall left the application payload behind."
        }
        Write-Output "MSI lifecycle smoke passed: install $Version, major-upgrade to $upgradeVersion, uninstall."
    }
    finally {
        $removeSmokeDirectory = $true
        if ($installer) {
            try {
                $remainingProductCodes = @(Get-RelatedProductCodes -Installer $installer -UpgradeCode $smokeUpgradeCode)
                foreach ($productCode in $remainingProductCodes) {
                    Invoke-MsiSmokeStep -MsiArguments @("/x", $productCode) `
                        -StepName "cleanup-$($productCode.Trim('{}'))" -LogDirectory $smokeDir
                }
            } catch {
                Write-Warning "Best-effort cleanup of MSI lifecycle smoke products failed: $_"
                $removeSmokeDirectory = $false
            }
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
        }
        if ($removeSmokeDirectory -and (Test-Path -LiteralPath $smokeInstallDirectory)) {
            Remove-Item -LiteralPath $smokeInstallDirectory -Recurse -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $smokeDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    exit 0
}

if (-not $CertBase64) {
    if ($SkipIfNoCert) {
        Write-Output "HEAT3_CODESIGN_CERT_BASE64 is not set; Authenticode signing skipped."
        exit 0
    }
    Write-Warning "HEAT3_CODESIGN_CERT_BASE64 is not set; binary will be packaged unsigned."
}

$resolved = Resolve-Path -LiteralPath $BinaryPath -ErrorAction SilentlyContinue
if (-not $resolved) {
    throw "Binary not found: $BinaryPath (run from the repo root after cargo build --release)"
}
$binary = $resolved.Path

if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME.StartsWith("v")) {
    $tagVersion = $env:GITHUB_REF_NAME.Substring(1)
    if ($Version -ne $tagVersion) {
        throw "Version $Version from Cargo.toml does not match tag $env:GITHUB_REF_NAME - refusing to sign/package."
    }
}

$cert = $null
$pfxPath = Join-Path $env:TEMP ("heat3-codesign-" + [guid]::NewGuid().ToString("N") + ".pfx")
function Invoke-Sign {
    param([string]$Path)
    if (-not $cert) {
        Write-Warning "Skipping signature for $Path (no certificate)."
        return
    }
    $signature = Set-AuthenticodeSignature -FilePath $Path -Certificate $cert -TimestampServer $TimestampServer
    if ($signature.Status -ne "Valid") {
        throw "Authenticode signing failed for $Path : $($signature.Status) ($($signature.StatusMessage))"
    }
    $verify = Get-AuthenticodeSignature -FilePath $Path
    if ($verify.Status -ne "Valid") {
        throw "Post-signature verification failed for $Path : $($verify.Status)"
    }
    Write-Output "Signed: $Path"
}

try {
    if ($CertBase64) {
        [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($CertBase64))
        # EphemeralKeySet keeps the imported private key in memory instead of
        # persisting it to the key store; Exportable is intentionally omitted.
        $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2(
            $pfxPath,
            $CertPassword,
            [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
        )
        Write-Output "Certificate loaded: thumbprint $($cert.Thumbprint)"
    }

    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    Invoke-Sign -Path $binary

    $stamp = Get-Date -Format "yyyyMMdd"
    $archive = Join-Path $OutDir "heat3_povorotnik-$Version-$stamp.zip"
    Compress-Archive -LiteralPath $binary -DestinationPath $archive -Force
    Write-Output "Package: $archive"

    if (Resolve-WixToolset) {
        $msiPath = Join-Path $OutDir "heat3_povorotnik-$Version.msi"
        Build-Msi -Binary $binary -Destination $msiPath -PackageVersion $Version
        Invoke-Sign -Path $msiPath
        Write-Output "MSI: $msiPath"
    } elseif ($RequireMsi) {
        throw "WiX toolset not found, but this release is required to ship an MSI."
    } else {
        Write-Output "WiX toolset not found; MSI packaging skipped (installer hook W8)."
    }
}
finally {
    Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue
    if ($cert) {
        $cert.Dispose()
    }
}
