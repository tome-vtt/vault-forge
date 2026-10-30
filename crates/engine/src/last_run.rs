//! Last run: the record of a profile's most recent conversion, the only one kept. Each
//! conversion that runs to its end replaces it; a cancelled one leaves it as it was.
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// The record of a profile's most recent conversion: its counts, warnings and log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    /// When the conversion started.
    pub started: SystemTime,
    /// How long it took, in whole seconds.
    pub seconds: u64,
    /// Whether its notes replaced the vault's converted notes.
    pub converted: bool,
    /// The converter's exit code; `None` when it was stopped from outside.
    pub exit_code: Option<i32>,
    /// How it ended, in one sentence.
    pub message: String,
    /// The likely cause of a failure the user can fix, when one is known.
    pub fix: Option<Fix>,
    /// The books chosen, adventures included.
    pub books: usize,
    pub homebrew: usize,
    /// The notes written, by vault folder (`compendium/bestiary`; notes directly in the
    /// compendium or rules folder under its own name), largest first.
    pub folders: Vec<(String, usize)>,
    pub warnings: Vec<Warning>,
    /// Every line the converter printed, in order.
    pub log: Vec<String>,
}

/// A warning line from the log, and where it came from: the step the converter was on, which is
/// the last ordinary line before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub line: String,
    pub step: String,
}

/// What the user can check to fix a failed conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fix {
    /// The converter could not read the game data: the address may not be the right data.
    DownloadAddress,
    /// The notes could not be put into the vault: a file in it may be open or read-only.
    Vault,
}

/// How the log marks a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warning,
    Info,
}

/// Errors and warnings as the converter prints them (`🛑  ERR|`, `🔸 WARN|`), or starting
/// `Error`/`Warning`.
pub fn log_level(line: &str) -> LogLevel {
    let level = line.split_once('|').map_or("", |(level, _)| level);
    let lower = line.to_lowercase();
    if level.ends_with("ERR") || lower.starts_with("error") {
        LogLevel::Error
    } else if level.ends_with("WARN") || lower.starts_with("warn") {
        LogLevel::Warning
    } else {
        LogLevel::Info
    }
}

impl LastRun {
    /// Every note written.
    pub fn notes(&self) -> usize {
        self.folders.iter().map(|(_, n)| n).sum()
    }

    /// The failure in a few words, for a heading.
    pub fn headline(&self) -> &'static str {
        match (self.exit_code, self.notes()) {
            (Some(0), _) => "The notes could not be put into your vault",
            (_, 0) => "The converter stopped before writing anything",
            _ => "The converter stopped partway",
        }
    }
}

/// The warnings in the log, each with the step it came from.
pub(crate) fn warnings(log: &[String]) -> Vec<Warning> {
    let mut step = "";
    let mut warnings = Vec::new();
    for line in log {
        match log_level(line) {
            LogLevel::Warning => warnings.push(Warning {
                line: line.clone(),
                step: step.to_owned(),
            }),
            LogLevel::Info => step = line,
            LogLevel::Error => {}
        }
    }
    warnings
}
