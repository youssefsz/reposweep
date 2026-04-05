use std::env;
use std::path::{Path, PathBuf};

use miette::{Context, IntoDiagnostic, Result};
use reposweep_core::{ConfigService, FileConfigStore};

pub fn run() -> Result<()> {
    let current_exe = env::current_exe().into_diagnostic()?;
    let install_dir = current_exe
        .parent()
        .map(Path::to_path_buf)
        .wrap_err("Failed to resolve install directory")?;
    let config_paths = ConfigService::new(FileConfigStore)
        .paths()
        .into_diagnostic()?;
    let ui_state_paths = FileConfigStore::ui_state_paths().into_diagnostic()?;

    println!("RepoSweep doctor");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!("Binary: {}", current_exe.display());
    println!("Install dir: {}", install_dir.display());
    println!(
        "Install dir on PATH: {}",
        yes_no(path_contains_dir(&install_dir))
    );
    println!("Platform: {} {}", env::consts::OS, env::consts::ARCH);
    println!("Config dir: {}", config_paths.config_dir.display());
    println!("Config file: {}", config_paths.config_file.display());
    println!(
        "Config exists: {}",
        yes_no(config_paths.config_file.exists())
    );
    println!("State dir: {}", ui_state_paths.data_dir.display());
    println!("State file: {}", ui_state_paths.state_file.display());
    println!(
        "State exists: {}",
        yes_no(ui_state_paths.state_file.exists())
    );

    Ok(())
}

fn path_contains_dir(candidate: &Path) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path).any(|entry| paths_match(&entry, candidate))
}

fn paths_match(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
