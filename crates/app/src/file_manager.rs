//! Opens the OS file manager on a file the Engine names, as the screens open OS file dialogs, and
//! hands links such as `obsidian://` to the app the OS has for them.
use std::io;
use std::path::Path;
use std::process::Command;

/// Shows `file` in Explorer or Finder, selected; elsewhere opens the folder holding it.
pub(crate) fn show(file: &Path) -> io::Result<()> {
    spawn(open(file))
}

/// Opens the link in the app the OS has for its scheme.
pub(crate) fn open_link(link: &str) -> io::Result<()> {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = Command::new(opener);
    command.arg(link);
    spawn(command)
}

/// The link that opens the vault in Obsidian.
pub(crate) fn obsidian_link(vault: &Path) -> String {
    let mut link = String::from("obsidian://open?path=");
    for byte in vault.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                link.push(byte as char)
            }
            _ => link.push_str(&format!("%{byte:02X}")),
        }
    }
    link
}

fn spawn(mut command: Command) -> io::Result<()> {
    let mut child = command.spawn()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_obsidian_link_carries_the_vault_path_percent_encoded() {
        assert_eq!(
            obsidian_link(Path::new("D:\\Obsidian\\Thursday Campaign")),
            "obsidian://open?path=D%3A%5CObsidian%5CThursday%20Campaign"
        );
        assert_eq!(
            obsidian_link(Path::new("/home/sam/Café & co")),
            "obsidian://open?path=%2Fhome%2Fsam%2FCaf%C3%A9%20%26%20co"
        );
    }
}
