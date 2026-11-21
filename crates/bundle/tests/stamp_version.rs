//! The release workflow's version stamp, `installer/stamp-version.ps1`: the tag sets the shipped
//! version. Runs the real script with PowerShell 7 (`pwsh`), which every GitHub runner has.
use std::path::Path;
use std::process::{Command, Output};

const CARGO_TOML: &str = "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.2.0\"\nedition = \"2024\"\n\n[workspace.dependencies]\nserde = { version = \"1.0.0\" }\n";

fn stamp(tag: &str) -> (Output, String, String, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let manifest = dir.join("Cargo.toml");
    std::fs::write(&manifest, CARGO_TOML).unwrap();
    let github_env = dir.join("github_env");
    std::fs::write(&github_env, "").unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../installer/stamp-version.ps1");
    let output = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(&script)
        .arg(&manifest)
        .env("RELEASE_TAG", tag)
        .env("GITHUB_ENV", &github_env)
        .env("RUNNER_TEMP", dir)
        .output()
        .expect("pwsh runs");
    let manifest = std::fs::read_to_string(&manifest).unwrap();
    // PowerShell ends each GITHUB_ENV line with the platform's newline.
    let github_env = std::fs::read_to_string(&github_env).unwrap().replace("\r\n", "\n");
    (output, manifest, github_env, temp)
}

#[test]
fn a_release_tag_sets_the_workspace_version_and_the_installer_version() {
    let (output, manifest, github_env, dir) = stamp("v1.2.3");
    assert!(output.status.success(), "{output:?}");
    assert!(manifest.contains("[workspace.package]\nversion = \"1.2.3\"\n"), "{manifest}");
    assert!(manifest.contains("serde = { version = \"1.0.0\" }"), "{manifest}");
    assert!(github_env.contains("VERSION=1.2.3\n"), "{github_env}");
    assert!(github_env.contains("PRERELEASE=false\n"), "{github_env}");
    let installer = dir.path().join("Vault-Forge-Setup-1.2.3.exe");
    assert!(github_env.contains(&format!("INSTALLER={}\n", installer.display())), "{github_env}");
}

#[test]
fn a_pre_release_tag_ships_its_dotted_base_version_flagged_as_a_pre_release() {
    let (output, manifest, github_env, dir) = stamp("v1.2.3-rc.1");
    assert!(output.status.success(), "{output:?}");
    assert!(manifest.contains("[workspace.package]\nversion = \"1.2.3\"\n"), "{manifest}");
    assert!(github_env.contains("VERSION=1.2.3\n"), "{github_env}");
    assert!(github_env.contains("PRERELEASE=true\n"), "{github_env}");
    let installer = dir.path().join("Vault-Forge-Setup-1.2.3-rc.1.exe");
    assert!(github_env.contains(&format!("INSTALLER={}\n", installer.display())), "{github_env}");
}

#[test]
fn a_malformed_tag_is_refused_and_nothing_is_stamped() {
    for tag in ["1.2.3", "v1.2", "v1.2.3.4", "vnext", "v1.2.3-", "v1.2.3 $(whoami)"] {
        let (output, manifest, github_env, _) = stamp(tag);
        assert!(!output.status.success(), "{tag} was accepted");
        let said = String::from_utf8_lossy(&output.stderr);
        assert!(said.contains("is not a release tag"), "{tag}: {said}");
        assert_eq!(manifest, CARGO_TOML, "{tag}");
        assert_eq!(github_env, "", "{tag}");
    }
}
