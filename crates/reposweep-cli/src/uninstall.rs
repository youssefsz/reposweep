use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use miette::{IntoDiagnostic, Result, miette};

#[derive(Debug, Clone, Copy)]
pub struct UninstallOptions {
    pub yes: bool,
}

pub fn run(options: UninstallOptions) -> Result<()> {
    let current_exe = std::env::current_exe().into_diagnostic()?;

    if !options.yes && !confirm_uninstall(&current_exe)? {
        println!("Uninstall cancelled. RepoSweep is staying put.");
        return Ok(());
    }

    self_replace::self_delete().into_diagnostic()?;

    #[cfg(windows)]
    println!(
        "RepoSweep scheduled its own removal from {}. Close this process and it will vanish.",
        current_exe.display()
    );

    #[cfg(not(windows))]
    println!(
        "RepoSweep removed itself from {}. Tiny broom, big feelings.",
        current_exe.display()
    );

    Ok(())
}

fn confirm_uninstall(current_exe: &PathBuf) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(miette!(
            "Refusing to uninstall without confirmation in a non-interactive terminal. Re-run with --yes."
        ));
    }

    print!(
        "Uninstall RepoSweep from {}? [y/N]: ",
        current_exe.display()
    );
    io::stdout().flush().into_diagnostic()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).into_diagnostic()?;

    let answer = input.trim();
    Ok(matches!(answer, "y" | "Y" | "yes" | "YES" | "Yes"))
}
