//! Downloader adapter: fetches files over HTTPS with progress and cancel, and holds no rules.
use std::io::{self, Read, Write};
use std::time::Duration;

/// What a server says about a file without sending it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Head {
    pub(crate) etag: Option<String>,
    pub(crate) last_modified: Option<String>,
    pub(crate) size: Option<u64>,
}

pub(crate) enum Error {
    Cancelled,
    Failed(String),
}

fn head_of<B>(response: &ureq::http::Response<B>) -> Head {
    let header = |name| {
        let value = response.headers().get(name)?.to_str().ok()?;
        Some(value.to_owned())
    };
    Head {
        etag: header("etag"),
        last_modified: header("last-modified"),
        size: header("content-length").and_then(|s| s.parse().ok()),
    }
}

// A silent server fails the download rather than hanging it, so Cancel waits on one read at most.
fn agent() -> ureq::Agent {
    let wait = Some(Duration::from_secs(60));
    let config = ureq::Agent::config_builder()
        .timeout_connect(wait)
        .timeout_recv_response(wait)
        .timeout_per_read(wait)
        .build();
    ureq::Agent::new_with_config(config)
}

/// Asks for the file's headers only.
pub(crate) fn head(url: &str) -> Result<Head, String> {
    let response = agent().head(url).call().map_err(|e| e.to_string())?;
    Ok(head_of(&response))
}

/// Streams the file into `out`, calling `progress` with the bytes so far after each chunk;
/// `progress` returning false cancels.
pub(crate) fn get(
    url: &str,
    out: &mut impl Write,
    mut progress: impl FnMut(u64) -> bool,
) -> Result<Head, Error> {
    let failed = |e: &dyn std::fmt::Display| Error::Failed(e.to_string());
    let response = agent().get(url).call().map_err(|e| failed(&e))?;
    let head = head_of(&response);
    let mut body = response.into_body().into_reader();
    let mut buf = vec![0; 64 * 1024];
    let mut bytes = 0;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(failed(&e)),
        };
        out.write_all(&buf[..n]).map_err(|e| failed(&e))?;
        bytes += n as u64;
        if !progress(bytes) {
            return Err(Error::Cancelled);
        }
    }
    Ok(head)
}
