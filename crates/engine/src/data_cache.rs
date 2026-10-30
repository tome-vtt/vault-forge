//! Data cache adapter: one folder per download address under `data-cache/`, holding the unpacked
//! archive and `forge.yaml` (what was downloaded and when). Holds no rules.
//! While a download runs, `<key>.part` is the archive and `<key>.new` the unpacked copy; a removed
//! copy waits in `data-cache.removing/` until it is deleted.
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::Book;

/// What the cache remembers about one downloaded copy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Cached {
    pub(crate) address: String,
    pub(crate) etag: Option<String>,
    pub(crate) last_modified: Option<String>,
    pub(crate) bytes: u64,
    /// Seconds since the Unix epoch.
    pub(crate) downloaded: u64,
    /// Read from the copy's own lists, not stored.
    #[serde(skip)]
    pub(crate) books: Vec<Book>,
}

const META: &str = "forge.yaml";

pub(crate) struct DataCache {
    dir: PathBuf,
}

impl DataCache {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let dir = app_data.join("data-cache");
        fs::create_dir_all(&dir)?;
        // Copies a closed app had not finished removing.
        let bin = dir.with_extension("removing");
        std::thread::spawn(move || {
            for entry in fs::read_dir(bin).into_iter().flatten().flatten() {
                let _ = remove_dir(&entry.path());
            }
        });
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

    /// Where the address's unpacked copy is.
    pub(crate) fn folder(&self, address: &str) -> PathBuf {
        self.path(address, "")
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

    /// Unpacks the downloaded archive beside the current copy and returns where. A file that is
    /// not a zip archive fails with [`io::ErrorKind::InvalidData`].
    pub(crate) fn unpack(&self, address: &str) -> io::Result<PathBuf> {
        let into = self.path(address, ".new");
        remove_dir(&into)?;
        let archive = File::open(self.path(address, ".part"))?;
        zip::ZipArchive::new(archive)
            .and_then(|mut zip| zip.extract(&into))
            .map_err(|e| match e {
                zip::result::ZipError::Io(e) => e,
                e => io::Error::new(io::ErrorKind::InvalidData, e),
            })?;
        Ok(into)
    }

    /// Puts the unpacked copy in place of the current one, which moves to `<key>.old` for
    /// [`DataCache::discard`]. Renames only, so a copy is never half removed.
    pub(crate) fn replace(&self, copy: &Cached) -> io::Result<()> {
        let new = self.path(&copy.address, ".new");
        let meta = serde_yaml_ng::to_string(copy).map_err(io::Error::other)?;
        fs::write(new.join(META), meta)?;
        let (current, old) = (
            self.path(&copy.address, ""),
            self.path(&copy.address, ".old"),
        );
        remove_dir(&old)?;
        let had_copy = current.exists();
        if had_copy {
            fs::rename(&current, &old)?;
        }
        fs::rename(new, &current).inspect_err(|_| {
            if had_copy {
                let _ = fs::rename(&old, &current);
            }
        })
    }

    /// Removes what a download left behind: its archive, an unfinished unpack and a replaced copy.
    pub(crate) fn discard(&self, address: &str) -> io::Result<()> {
        remove_dir(&self.path(address, ".new"))?;
        remove_dir(&self.path(address, ".old"))?;
        match fs::remove_file(self.path(address, ".part")) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Removes the address's copy: moved out of the cache at once, deleted on a worker thread,
    /// since a large copy takes a while.
    pub(crate) fn remove(&self, address: &str) -> io::Result<()> {
        let current = self.path(address, "");
        if !current.exists() {
            return Ok(());
        }
        let bin = self.dir.with_extension("removing");
        fs::create_dir_all(&bin)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let doomed = bin.join(nanos.to_string());
        fs::rename(current, &doomed)?;
        std::thread::spawn(move || remove_dir(&doomed));
        Ok(())
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
