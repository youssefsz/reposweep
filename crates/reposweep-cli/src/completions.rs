use std::io;

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::Cli;

pub fn run(shell: Shell) -> miette::Result<()> {
    let mut command = Cli::command();
    let command_name = command.get_name().to_string();
    let mut stdout = io::stdout();
    generate(shell, &mut command, command_name, &mut stdout);
    Ok(())
}
