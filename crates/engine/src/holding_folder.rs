//! The holding folder adapter: creates the folder a conversion writes into, swaps folders from it
//! into the vault, or discards it. No rules: the conversion decides which.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Inside the vault, so the swap is a rename on one disk; Obsidian leaves dot folders alone.
const NAME: &str = ".vault-forge-holding";

/// An empty holding folder in the vault, clearing any a stopped app left behind.
pub(crate) fn create(vault: &Path) -> io::Result<PathBuf> {
    let holding = vault.join(NAME);
    discard(&holding)?;
    fs::create_dir_all(&holding)?;
    Ok(holding)
}

/// Replaces each of the vault's `folders` with the holding folder's, moving the old ones into the
/// holding folder. A folder the holding folder lacks is removed from the vault. On an error, every
/// folder already moved is put back and the vault is as it was.
pub(crate) fn swap(holding: &Path, vault: &Path, folders: &[String]) -> io::Result<()> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut rename = |from: PathBuf, to: PathBuf| {
        fs::rename(&from, &to)?;
        moved.push((from, to));
        io::Result::Ok(())
    };
    let outcome = folders.iter().enumerate().try_for_each(|(i, folder)| {
        let (old, new) = (vault.join(folder), holding.join(folder));
        if old.exists() {
            rename(old.clone(), holding.join(format!(".replaced-{i}")))?;
        }
        if new.exists() {
            fs::create_dir_all(old.parent().unwrap_or(vault))?;
            rename(new, old)?;
        }
        Ok(())
    });
    if let Err(e) = outcome {
        for (from, to) in moved.into_iter().rev() {
            let _ = fs::rename(to, from);
        }
        return Err(e);
    }
    Ok(())
}

/// Removes the holding folder and everything in it.
pub(crate) fn discard(holding: &Path) -> io::Result<()> {
    match fs::remove_dir_all(holding) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
