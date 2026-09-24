[CmdletBinding()]
param(
    [string]$Root = (Join-Path $PSScriptRoot '..')
)

$resolvedRoot = (Resolve-Path -LiteralPath $Root).Path

function Get-ProjectFiles {
    param([Parameter(Mandatory = $true)][string]$Directory)

    foreach ($item in Get-ChildItem -LiteralPath $Directory -Force) {
        if ($item.PSIsContainer) {
            if ($item.Name -in @('.git', 'target')) {
                continue
            }
            Get-ProjectFiles -Directory $item.FullName
        }
        else {
            $item
        }
    }
}

$files = @(Get-ProjectFiles -Directory $resolvedRoot) |
    Sort-Object -Property FullName -Unique

$conflicts = @($files | Where-Object { $_.Name -like '*.sync-conflict-*' })
if ($conflicts) {
    $paths = ($conflicts.FullName | ForEach-Object { "  $_" }) -join [Environment]::NewLine
    throw "Sync-conflict files found:$([Environment]::NewLine)$paths"
}

# Fail-closed guard for credential containers and private-key material. These
# must never live in the repository, not even force-added past .gitignore.
$credentialExtensions = @('.pfx', '.p12', '.pem', '.key', '.p8')
$credentialFiles = @($files | Where-Object {
        $name = $_.Name
        $isEnvSecret = ($name -eq '.env' -or $name -like '.env.*') -and
        $name -notlike '*.example' -and $name -notlike '*.sample'
        $credentialExtensions -contains $_.Extension.ToLowerInvariant() -or $isEnvSecret
    })
if ($credentialFiles) {
    $paths = ($credentialFiles.FullName | ForEach-Object { "  $_" }) -join [Environment]::NewLine
    throw "Credential or private-key files must not be committed:$([Environment]::NewLine)$paths"
}

$sourceExtensions = @('.rs', '.toml', '.md', '.txt', '.yml', '.yaml', '.ps1', '.json', '.lock')
$windows1251 = [Text.Encoding]::GetEncoding(1251)
$strictUtf8 = [Text.UTF8Encoding]::new($false, $true)
$mojibakeMarker = '(?:[\u0420\u0421\u0432][\u0400-\u040F\u0450-\u045F\u00A0-\u00BF\u2018-\u2026]|\u0440\u045F)'
$encodingProblems = foreach ($file in $files) {
    if ($sourceExtensions -notcontains $file.Extension) {
        continue
    }
    try {
        $lines = [IO.File]::ReadAllLines($file.FullName, $strictUtf8)
    }
    catch {
        "$($file.FullName): invalid UTF-8"
        continue
    }
    for ($index = 0; $index -lt $lines.Length; $index++) {
        $line = $lines[$index]
        if ($line -cnotmatch '[^\x00-\x7F]') {
            continue
        }
        if ($line -cmatch $mojibakeMarker) {
            "$($file.FullName):$($index + 1):$($line.Trim())"
            continue
        }
        try {
            $bytes = $windows1251.GetBytes($line)
            if ($windows1251.GetString($bytes) -cne $line) {
                continue
            }
            $decoded = $strictUtf8.GetString($bytes)
            if ($decoded -cne $line) {
                "$($file.FullName):$($index + 1):$($line.Trim())"
            }
        }
        catch {
            continue
        }
    }
}
if ($encodingProblems) {
    $details = ($encodingProblems | ForEach-Object { "  $_" }) -join [Environment]::NewLine
    throw "Possible UTF-8 mojibake found:$([Environment]::NewLine)$details"
}

Write-Output "Source hygiene check passed."
