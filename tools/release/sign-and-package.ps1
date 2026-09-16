# Signs the release binary with an Authenticode certificate, packages it as a
# ZIP and builds an MSI installer with the WiX toolset (architecture plan
# section 8.1, W8).
#
# Without HEAT3_CODESIGN_CERT_BASE64 the script skips signing (and exits 0 with
# -SkipIfNoCert) so local runs and CI without a certificate stay green. Without
# the WiX toolset the MSI step is skipped with a warning.
#
# Usage (from repo root):
#   .\tools\release\sign-and-package.ps1 [-SkipIfNoCert] [-WixRoot C:\...\WiX Toolset v3.14]
# Env: HEAT3_CODESIGN_CERT_BASE64, HEAT3_CODESIGN_CERT_PASSWORD
param(
    [string]$BinaryPath = "target\release\heat3_povorotnik.exe",
    [string]$OutDir = "dist",
    [string]$Version = "",
    [string]$CertBase64 = $env:HEAT3_CODESIGN_CERT_BASE64,
    [string]$CertPassword = $env:HEAT3_CODESIGN_CERT_PASSWORD,
    [string]$TimestampServer = "http://timestamp.digicert.com",
    [string]$WixRoot = $env:WIX,
    [switch]$SkipIfNoCert
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$wxsPath = Join-Path $scriptDir "heat3_povorotnik.wxs"

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

if (-not $Version) {
    $match = Select-String -LiteralPath "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1
    if ($match) {
        $Version = $match.Matches[0].Groups[1].Value
    } else {
        $Version = "0.0.0-local"
    }
}

if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME.StartsWith("v")) {
    $tagVersion = $env:GITHUB_REF_NAME.Substring(1)
    if ($Version -ne $tagVersion) {
        throw "Version $Version from Cargo.toml does not match tag $env:GITHUB_REF_NAME — refusing to sign/package."
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
        $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2(
            $pfxPath,
            $CertPassword,
            [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::Exportable
        )
        Write-Output "Certificate loaded: thumbprint $($cert.Thumbprint)"
    }

    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    Invoke-Sign -Path $binary

    $stamp = Get-Date -Format "yyyyMMdd"
    $archive = Join-Path $OutDir "heat3_povorotnik-$Version-$stamp.zip"
    Compress-Archive -LiteralPath $binary -DestinationPath $archive -Force
    Write-Output "Package: $archive"

    $candle = $null
    $light = $null
    foreach ($root in @($WixRoot, "C:\Program Files (x86)\WiX Toolset v3.14")) {
        if (-not $root) { continue }
        $c = Join-Path $root "bin\candle.exe"
        $l = Join-Path $root "bin\light.exe"
        if ((Test-Path -LiteralPath $c) -and (Test-Path -LiteralPath $l)) {
            $candle = $c
            $light = $l
            break
        }
    }
    if ($candle) {
        $msiPath = Join-Path $OutDir "heat3_povorotnik-$Version.msi"
        $wixOut = Join-Path $env:TEMP ("heat3-wix-" + [guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Force -Path $wixOut | Out-Null
        try {
            & $candle -arch x64 "-dHeat3Exe=$binary" "-dHeat3Version=$Version" -out "$wixOut\" $wxsPath
            if ($LASTEXITCODE -ne 0) {
                throw "candle failed with exit code $LASTEXITCODE"
            }
            & $light -out $msiPath (Join-Path $wixOut "heat3_povorotnik.wixobj")
            if ($LASTEXITCODE -ne 0) {
                throw "light failed with exit code $LASTEXITCODE"
            }
            Invoke-Sign -Path $msiPath
            Write-Output "MSI: $msiPath"
        }
        finally {
            Remove-Item -LiteralPath $wixOut -Recurse -Force -ErrorAction SilentlyContinue
        }
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
