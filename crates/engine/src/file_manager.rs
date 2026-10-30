//! File manager adapter: opens the OS file manager on a file. Holds no rules.
use std::io;
use std::path::Path;
use std::process::Command;

/// Shows `file` in Explorer or Finder, selected; elsewhere opens the folder holding it.
pub(crate) fn show(file: &Path) -> io::Result<()> {
    let mut child = open(file).spawn()?;
    // Reaped on its own thread so the screen never waits on the file manager.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(windows)]
fn open(file: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("explorer");
    // Explorer reads `/select,"path"` itself, so the usual argument quoting would break it.
    command.raw_arg(format!("/select,\"{}\"", file.display()));
    command
}

#[cfg(target_os = "macos")]
fn open(file: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(file);
    command
}

#[cfg(not(any(windows, target_os = "macos")))]
fn open(file: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(file.parent().unwrap_or(file));
    command
}
