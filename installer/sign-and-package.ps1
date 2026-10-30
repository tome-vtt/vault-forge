# Signs the app in target/payload, builds and signs the Silk Setup installer around it, then
# verifies both signatures. Windows only; run by .github/workflows/release.yml after azure/login.
# Needs VERSION, INSTALLER, GH_TOKEN (the SILK_SETUP_TOKEN secret) and the AZURE_CODESIGNING_*
# variables.
$ErrorActionPreference = 'Stop'
$silkVersion = 'v0.2.1'
# TomeVTT's verified v0.2.1 hashes; never trust a list downloaded from the same mutable release.
$silkHashes = @{
    'silk.exe' = 'fae87c244ad0e2aa227f054c2ddc4557910fea6f7b0b5033ec939d6505cee5ad'
    'silk-stub.exe' = 'c484ac2456fbe56014f899cdf3c9568b20fb3bcee3156d539c9124cda93a5a07'
}
$app = Join-Path $PSScriptRoot '../target/payload/vault-forge.exe' | Resolve-Path

Install-Module -Name TrustedSigning -RequiredVersion 0.5.0 -Force -Repository PSGallery -Scope CurrentUser
Invoke-TrustedSigning -Endpoint $env:AZURE_CODESIGNING_ENDPOINT -CodeSigningAccountName $env:AZURE_CODESIGNING_ACCOUNT `
    -CertificateProfileName $env:AZURE_CODESIGNING_PROFILE -Files $app.Path `
    -FileDigest SHA256 -TimestampRfc3161 http://timestamp.acs.microsoft.com -TimestampDigest SHA256

$silk = Join-Path $env:RUNNER_TEMP 'silk'
gh release download $silkVersion --repo tome-vtt/silk-setup --dir $silk --pattern silk.exe --pattern silk-stub.exe
if ($LASTEXITCODE -ne 0) { throw 'Could not download Silk Setup' }
foreach ($name in $silkHashes.Keys) {
    $actual = (Get-FileHash (Join-Path $silk $name) -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $silkHashes[$name]) { throw "Checksum mismatch for $name" }
}

# Silk signs the uninstaller and the installer itself, through vault-forge.toml's [sign].
& (Join-Path $silk 'silk.exe') build (Join-Path $PSScriptRoot 'vault-forge.toml') --version $env:VERSION --out $env:INSTALLER
if ($LASTEXITCODE -ne 0) { throw 'silk build failed' }

# An unsigned file fails the release here, before anything is published.
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe |
    Where-Object { $_.FullName -like '*x64*' } | Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe not found on the runner' }
foreach ($file in $app.Path, $env:INSTALLER) {
    & $signtool.FullName verify /pa /v $file
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $file" }
}
