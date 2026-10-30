//! Data cache adapter: one folder per download address under `data-cache/`, holding the unpacked
//! archive and `forge.yaml` (what was downloaded and when). Holds no rules.
//! While a download runs, `<key>.part` is the archive and `<key>.new` the unpacked copy.
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What the cache remembers about one downloaded copy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Cached {
    pub(crate) address: String,
    pub(crate) etag: Option<String>,
    pub(crate) last_modified: Option<String>,
    pub(crate) bytes: u64,
    pub(crate) books: usize,
    /// Seconds since the Unix epoch.
    pub(crate) downloaded: u64,
}

const META: &str = "forge.yaml";

pub(crate) struct DataCache {
    dir: PathBuf,
}

impl DataCache {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let dir = app_data.join("data-cache");
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// The address's folder: a hash, since an address can be long and full of characters no file
    /// system allows.
    fn path(&self, address: &str, suffix: &str) -> PathBuf {
        // FNV-1a: std's hasher is not promised to stay the same between Rust releases.
        let hash = address.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        self.dir.join(format!("{hash:016x}{suffix}"))
    }

    /// The copy for the address, if one was downloaded.
    pub(crate) fn read(&self, address: &str) -> Option<Cached> {
        let text = fs::read_to_string(self.path(address, "").join(META)).ok()?;
        serde_yaml_ng::from_str(&text).ok()
    }

    /// A fresh, empty file to download the archive into.
    pub(crate) fn archive(&self, address: &str) -> io::Result<File> {
        File::create(self.path(address, ".part"))
    }

    /// Unpacks the downloaded archive beside the current copy and returns where.
    pub(crate) fn unpack(&self, address: &str) -> io::Result<PathBuf> {
        let into = self.path(address, ".new");
        remove_dir(&into)?;
        let archive = File::open(self.path(address, ".part"))?;
        zip::ZipArchive::new(archive)
            .and_then(|mut zip| zip.extract(&into))
            .map_err(io::Error::other)?;
        Ok(into)
    }

    /// Puts the unpacked copy in place of the current one.
    pub(crate) fn replace(&self, copy: &Cached) -> io::Result<()> {
        let new = self.path(&copy.address, ".new");
        let meta = serde_yaml_ng::to_string(copy).map_err(io::Error::other)?;
        fs::write(new.join(META), meta)?;
        let current = self.path(&copy.address, "");
        remove_dir(&current)?;
        fs::rename(new, current)
    }

    /// Removes what an unfinished download left behind.
    pub(crate) fn discard(&self, address: &str) -> io::Result<()> {
        remove_dir(&self.path(address, ".new"))?;
        match fs::remove_file(self.path(address, ".part")) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Removes the address's copy.
    pub(crate) fn remove(&self, address: &str) -> io::Result<()> {
        remove_dir(&self.path(address, ""))
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
