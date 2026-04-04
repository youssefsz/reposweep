use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::domain::{ArtifactKind, DeleteStrategy};
use crate::error::{Result, ShatterError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomRuleConfig {
    pub name: String,
    pub directory_name: String,
    pub kind: ArtifactKind,
    pub ecosystem: String,
    #[serde(default)]
    pub required_markers: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub disabled_ecosystems: Vec<String>,
    #[serde(default)]
    pub protected_paths: Vec<PathBuf>,
    #[serde(default)]
    pub custom_rules: Vec<CustomRuleConfig>,
    #[serde(default)]
    pub default_delete_strategy: DeleteStrategy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            disabled_ecosystems: vec![],
            protected_paths: vec![
                PathBuf::from(".git"),
                PathBuf::from(".github"),
                PathBuf::from(".idea/runConfigurations"),
            ],
            custom_rules: vec![],
            default_delete_strategy: DeleteStrategy::Trash,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug)]
pub struct ConfigPaths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct UiStatePaths {
    pub data_dir: PathBuf,
    pub state_file: PathBuf,
}

pub trait ConfigStore {
    fn load(&self) -> Result<Config>;
    fn save(&self, config: &Config) -> Result<()>;
    fn paths(&self) -> Result<ConfigPaths>;
}

#[derive(Clone, Debug, Default)]
pub struct FileConfigStore;

impl FileConfigStore {
    pub fn ui_state_paths() -> Result<UiStatePaths> {
        let project_dirs = project_dirs()?;
        let data_dir = project_dirs.data_dir().to_path_buf();
        Ok(UiStatePaths {
            state_file: data_dir.join("state.toml"),
            data_dir,
        })
    }
}

impl ConfigStore for FileConfigStore {
    fn load(&self) -> Result<Config> {
        let paths = self.paths()?;
        if !paths.config_file.exists() {
            return Ok(Config::default());
        }

        let contents = fs::read_to_string(&paths.config_file)
            .map_err(|error| ShatterError::io("read config", &paths.config_file, error))?;
        toml::from_str(&contents)
            .map_err(|error| ShatterError::Config(format!("failed to parse config: {error}")))
    }

    fn save(&self, config: &Config) -> Result<()> {
        let paths = self.paths()?;
        fs::create_dir_all(&paths.config_dir)
            .map_err(|error| ShatterError::io("create config dir", &paths.config_dir, error))?;
        let contents = toml::to_string_pretty(config).map_err(|error| {
            ShatterError::Config(format!("failed to serialize config: {error}"))
        })?;
        fs::write(&paths.config_file, contents)
            .map_err(|error| ShatterError::io("write config", &paths.config_file, error))
    }

    fn paths(&self) -> Result<ConfigPaths> {
        let project_dirs = project_dirs()?;
        let config_dir = project_dirs.config_dir().to_path_buf();
        Ok(ConfigPaths {
            config_file: config_dir.join("config.toml"),
            config_dir,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ConfigService<S> {
    store: S,
}

impl<S> ConfigService<S>
where
    S: ConfigStore,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Config> {
        self.store.load()
    }

    pub fn load_or_default(&self) -> Config {
        self.store.load().unwrap_or_default()
    }

    pub fn save_default(&self) -> Result<ConfigPaths> {
        let config = Config::default();
        self.store.save(&config)?;
        self.store.paths()
    }

    pub fn paths(&self) -> Result<ConfigPaths> {
        self.store.paths()
    }
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "Shatter", "shatter")
        .ok_or_else(|| ShatterError::Config("failed to resolve OS config directories".into()))
}

pub fn resolve_protected_path(root: &Path, protected: &Path) -> PathBuf {
    if protected.is_absolute() {
        protected.to_path_buf()
    } else {
        root.join(protected)
    }
}
