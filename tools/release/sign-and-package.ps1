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
# -SmokeTest performs a credential-free packaging smoke: it fabricates a dummy
# executable, runs Candle+Light and validates the resulting MSI without ever
# touching signing secrets. Intended for PR CI.
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
        [Parameter(Mandatory = $true)][string]$Destination
    )
    $wix = Resolve-WixToolset
    if (-not $wix) {
        throw "WiX toolset not found; MSI packaging is required here (installer hook W8)."
    }
    $wixOut = Join-Path $env:TEMP ("heat3-wix-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $wixOut | Out-Null
    try {
        & $wix.Candle -arch x64 "-dHeat3Exe=$Binary" "-dHeat3Version=$Version" -out "$wixOut\" $wxsPath
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

if ($SmokeTest) {
    # Credential-free validation that the MSI pipeline actually works: no
    # signing, no release binary, only Candle+Light against a dummy payload.
    if (-not (Resolve-WixToolset)) {
        Write-Output "WiX toolset not found; packaging smoke skipped (trusted releases still fail via -RequireMsi)."
        exit 0
    }
    $smokeDir = Join-Path $env:TEMP ("heat3-package-smoke-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $smokeDir | Out-Null
    try {
        $dummy = Join-Path $smokeDir "heat3_povorotnik.exe"
        [IO.File]::WriteAllBytes($dummy, [byte[]](0x4D, 0x5A) + [byte[]]::new(64))
        $msi = Join-Path $smokeDir "heat3_povorotnik-$Version-smoke.msi"
        Build-Msi -Binary $dummy -Destination $msi
        Write-Output "Package smoke OK: $msi"
    }
    finally {
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
        Build-Msi -Binary $binary -Destination $msiPath
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
