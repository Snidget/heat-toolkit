$ErrorActionPreference = "Stop"

$metadata = & cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$features = $metadata.packages | Where-Object { $_.name -eq "heat3_povorotnik" } | Select-Object -ExpandProperty features
if (-not ($features.PSObject.Properties.Name -contains "ui-iced")) {
    throw "ui-iced feature is missing from the client package"
}

function Assert-TargetGraphDoesNotContain {
    param(
        [Parameter(Mandatory = $true)][string]$Spec,
        [Parameter(Mandatory = $true)][string]$DisplayName
    )

    $treeOutput = & cargo tree --target x86_64-pc-windows-msvc -i $Spec 2>&1
    $treeExitCode = $LASTEXITCODE
    $tree = $treeOutput -join "`n"
    if ($treeExitCode -ne 0 -and $tree -notmatch "warning: nothing to print\.$") {
        throw "cargo tree failed while checking the Windows dependency graph for $DisplayName"
    }
    if ($tree -match "(?m)^$([regex]::Escape($DisplayName)) v") {
        throw "$DisplayName entered the Windows dependency graph; remove the RustSec exceptions"
    }
}

Assert-TargetGraphDoesNotContain -Spec "quick-xml" -DisplayName "quick-xml"
Assert-TargetGraphDoesNotContain -Spec "memmap2@0.5.10" -DisplayName "memmap2"

& cargo audit `
    --file Cargo.lock `
    --ignore RUSTSEC-2026-0194 `
    --ignore RUSTSEC-2026-0195 `
    --ignore RUSTSEC-2026-0186

if ($LASTEXITCODE -ne 0) {
    throw "cargo audit failed"
}
