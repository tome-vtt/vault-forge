//! The converter adapter: starts the converter program and hands back its output. No rules.
use std::io::{self, BufRead, BufReader, PipeReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::Rules;

/// Starts the converter on the config and the game data's `data` folder, writing into `output`.
/// Returns the running program and its stdout and stderr as one stream of lines, in the order it
/// printed them.
pub(crate) fn start(
    program: &Path,
    config: &Path,
    rules: Rules,
    data: &Path,
    output: &Path,
) -> io::Result<(Child, io::Lines<BufReader<PipeReader>>)> {
    let game = match rules {
        Rules::Dnd5e => "5e",
        Rules::Pf2e => "pf2e",
    };
    let (reader, writer) = io::pipe()?;
    let mut command = Command::new(program);
    command
        .arg("-c")
        .arg(config)
        .arg("-o")
        .arg(output)
        .args(["-g", game])
        .arg(data)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    #[cfg(windows)]
    {
        // No console window flashes up behind the app.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = command.spawn()?;
    // The pipe ends once the program exits only when no copy of its writing end is left here.
    drop(command);
    Ok((child, BufReader::new(reader).lines()))
}
