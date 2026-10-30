//! The holding folder adapter: creates the folder a conversion writes into, swaps folders from it
//! into the vault, or discards it. No rules: the conversion decides which.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The holding folder's name. It is inside the vault, so the swap is a rename on one disk;
/// Obsidian leaves dot folders alone.
pub(crate) const NAME: &str = ".vault-forge-holding";

/// The start of the name a vault folder takes in the holding folder while it is replaced.
const REPLACED: &str = ".replaced-";

/// An empty holding folder in the vault, clearing any a stopped app left behind, unless it still
/// holds old folders a swap could not put back.
pub(crate) fn create(vault: &Path) -> io::Result<PathBuf> {
    let holding = vault.join(NAME);
    let mut entries = fs::read_dir(&holding).into_iter().flatten().flatten();
    if entries.any(|e| e.file_name().to_string_lossy().starts_with(REPLACED)) {
        return Err(io::Error::other(format!(
            "{} still holds old notes that could not be put back; move them back into the vault \
             first",
            holding.display()
        )));
    }
    discard(&holding)?;
    fs::create_dir_all(&holding)?;
    Ok(holding)
}

/// Why a swap failed, and whether every folder it had moved was put back.
pub(crate) struct SwapFailed {
    pub(crate) error: io::Error,
    pub(crate) put_back: bool,
}

/// Replaces each of the vault's `folders` with the holding folder's, moving the old ones into the
/// holding folder. A folder the holding folder lacks is removed from the vault. On an error, every
/// folder already moved is put back; when that fails too, the old folders stay in the holding
/// folder.
pub(crate) fn swap(holding: &Path, vault: &Path, folders: &[String]) -> Result<(), SwapFailed> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut rename = |from: PathBuf, to: PathBuf| {
        fs::rename(&from, &to)?;
        moved.push((from, to));
        io::Result::Ok(())
    };
    let outcome = folders.iter().enumerate().try_for_each(|(i, folder)| {
        let (old, new) = (vault.join(folder), holding.join(folder));
        if old.exists() {
            rename(old.clone(), holding.join(format!("{REPLACED}{i}")))?;
        }
        if new.exists() {
            fs::create_dir_all(old.parent().unwrap_or(vault))?;
            rename(new, old)?;
        }
        Ok(())
    });
    outcome.map_err(|error| SwapFailed {
        error,
        put_back: moved
            .into_iter()
            .rev()
            .fold(true, |ok, (from, to)| fs::rename(to, from).is_ok() && ok),
    })
}

/// Removes the holding folder and everything in it.
pub(crate) fn discard(holding: &Path) -> io::Result<()> {
    match fs::remove_dir_all(holding) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
