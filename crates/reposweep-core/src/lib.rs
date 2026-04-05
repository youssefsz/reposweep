pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod rules;
pub mod services;

pub use config::{
    Config, ConfigPaths, ConfigService, ConfigStore, CustomRuleConfig, FileConfigStore,
    UiStatePaths,
};
pub use domain::{
    ArtifactKind, CancellationToken, DeleteFailure, DeleteRequest, DeleteResult, DeleteStrategy,
    ProtectionPolicy, ScanEvent, ScanItem, ScanReport, ScanRequest, ScanScope, ScanTotals,
    ScanWarning, SizeMode, format_bytes,
};
pub use error::{RepoSweepError, Result};
pub use infrastructure::{DeletionBackend, FsDeletionBackend, FsDirectoryWalker, ParallelSizer};
pub use rules::{Rule, RuleMatchKind, RuleSet, built_in_rules};
pub use services::{DeleteService, ScanService};
