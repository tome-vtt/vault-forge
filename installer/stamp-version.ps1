# Stamps the release tag's version into the workspace Cargo.toml, so the app, its file version and
# the installer all carry it; run by .github/workflows/release.yml before cargo build. Nothing is
# committed back: the Cargo.toml version on development is not the release version.
# Reads RELEASE_TAG (vX.Y.Z or vX.Y.Z-suffix), writes VERSION, INSTALLER and PRERELEASE to
# GITHUB_ENV. Tested by crates/bundle/tests/stamp_version.rs.
param([string] $Manifest = (Join-Path $PSScriptRoot '../Cargo.toml'))
$ErrorActionPreference = 'Stop'

$tag = $env:RELEASE_TAG
if ($tag -cnotmatch '^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))(-[0-9A-Za-z][0-9A-Za-z.-]*)?\z') {
    [Console]::Error.WriteLine("'$tag' is not a release tag: push a tag such as v1.2.3, or v1.2.3-rc.1 for a pre-release")
    exit 1
}
# Silk and the app's release check take dotted numbers only, so a pre-release ships its base version.
$version = $Matches[1]
$prerelease = [bool] $Matches[2]

$text = [IO.File]::ReadAllText($Manifest)
$line = '(?m)^version = "[^"]*"'
if ([regex]::Matches($text, $line).Count -ne 1) { throw "Expected one workspace version line in $Manifest" }
[IO.File]::WriteAllText($Manifest, ([regex] $line).Replace($text, "version = `"$version`"", 1))

"VERSION=$version" >> $env:GITHUB_ENV
"INSTALLER=$(Join-Path $env:RUNNER_TEMP "Vault-Forge-Setup-$($tag.Substring(1)).exe")" >> $env:GITHUB_ENV
"PRERELEASE=$($prerelease.ToString().ToLower())" >> $env:GITHUB_ENV
