use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shatter_core::{FileConfigStore, Result as CoreResult, ShatterError};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub recent_paths: Vec<PathBuf>,
    pub last_browser_path: Option<PathBuf>,
}

pub fn load() -> AppState {
    load_inner().unwrap_or_default()
}

pub fn save(state: &AppState) -> CoreResult<()> {
    let paths = FileConfigStore::ui_state_paths()?;
    fs::create_dir_all(&paths.data_dir)
        .map_err(|error| ShatterError::io("create data dir", &paths.data_dir, error))?;
    let contents = toml::to_string_pretty(state)
        .map_err(|error| ShatterError::Config(format!("failed to serialize UI state: {error}")))?;
    fs::write(&paths.state_file, contents)
        .map_err(|error| ShatterError::io("write UI state", &paths.state_file, error))
}

fn load_inner() -> CoreResult<AppState> {
    let paths = FileConfigStore::ui_state_paths()?;
    if !paths.state_file.exists() {
        return Ok(AppState::default());
    }

    let contents = fs::read_to_string(&paths.state_file)
        .map_err(|error| ShatterError::io("read UI state", &paths.state_file, error))?;
    toml::from_str(&contents)
        .map_err(|error| ShatterError::Config(format!("failed to parse UI state: {error}")))
}
