//! Release check: asks GitHub for the newest Vault Forge release and reports it when newer.
//! It never installs anything, and a failed check is silent: the next launch asks again.
use std::sync::mpsc::Sender;

use serde::Deserialize;

use crate::{Event, VERSION, downloader};

const RELEASES_PAGE: &str = "https://github.com/tome-vtt/vault-forge/releases/";

/// The parts of GitHub's latest-release answer the check reads.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

pub(crate) fn check(url: String, events: Sender<Event>) {
    std::thread::spawn(move || {
        let Some(release) = latest(&url) else {
            return;
        };
        let version = release.tag_name.trim_start_matches('v');
        // The page opens in the browser, so only a Vault Forge release page is passed on.
        let ours = release.html_url.starts_with(RELEASES_PAGE);
        if ours && is_newer(version, VERSION) {
            let _ = events.send(Event::NewerRelease {
                version: version.to_owned(),
                page: release.html_url,
            });
        }
    });
}

fn latest(url: &str) -> Option<Release> {
    let mut body = Vec::new();
    downloader::get(url, &mut body, |_| true).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Compares dotted version numbers part by part; anything unreadable is never newer.
fn is_newer(version: &str, than: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> { v.split('.').map(|p| p.parse().ok()).collect() };
    parts(version).zip(parts(than)).is_some_and(|(a, b)| a > b)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("not-a-version", "1.0.0"));
    }
}
