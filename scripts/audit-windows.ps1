$ErrorActionPreference = "Stop"

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

# RUSTSEC-2026-0194 and RUSTSEC-2026-0195 affect quick-xml before 0.41.0.
# cargo-audit cannot filter by target, so both advisories are ignored only after
# proving that no quick-xml version is reachable from the Windows client graph.
Assert-TargetGraphDoesNotContain -Spec "quick-xml" -DisplayName "quick-xml"

& cargo audit `
    --file Cargo.lock `
    --ignore RUSTSEC-2026-0194 `
    --ignore RUSTSEC-2026-0195

if ($LASTEXITCODE -ne 0) {
    throw "cargo audit failed"
}
