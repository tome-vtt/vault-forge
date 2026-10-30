# Runs the real bundled converter once on the tiny sample in smoke/ and fails unless it exits 0
# and writes notes. Usage: pwsh installer/smoke-test.ps1 <folder holding the app and converter/>
param([Parameter(Mandatory)] [string] $AppFolder)
$ErrorActionPreference = 'Stop'

# Where the Engine looks for it (bundled_converter in crates/engine/src/lib.rs).
$converter = Join-Path $AppFolder "converter/ttrpg-convert$(if ($IsWindows) { '.exe' })"
if (-not (Test-Path $converter)) { throw "No converter at $converter" }

$smoke = Join-Path $PSScriptRoot 'smoke'
$out = Join-Path ([IO.Path]::GetTempPath()) "vault-forge-smoke-$([guid]::NewGuid())"
try {
    # The arguments the Engine passes (crates/engine/src/converter.rs).
    & $converter -c (Join-Path $smoke 'config.yaml') -o $out -g 5e (Join-Path $smoke 'data')
    if ($LASTEXITCODE -ne 0) { throw "The converter exited $LASTEXITCODE" }
    $notes = @(Get-ChildItem $out -Recurse -Filter '*.md' -ErrorAction SilentlyContinue)
    if ($notes.Count -eq 0) { throw 'The converter wrote no notes' }
    Write-Host "Smoke test passed: the converter wrote $($notes.Count) notes"
} finally {
    Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
}
