use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Cache,
    Dependency,
    #[default]
    Build,
}

impl Display for ArtifactKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Cache => "cache",
            Self::Dependency => "dependency",
            Self::Build => "build",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanScope {
    Cache,
    Dependencies,
    #[default]
    All,
}

impl ScanScope {
    pub fn includes(self, kind: ArtifactKind) -> bool {
        match self {
            Self::All => true,
            Self::Cache => matches!(kind, ArtifactKind::Cache | ArtifactKind::Build),
            Self::Dependencies => kind == ArtifactKind::Dependency,
        }
    }
}

impl Display for ScanScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Cache => "cache",
            Self::Dependencies => "dependencies",
            Self::All => "all",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionPolicy {
    #[default]
    RespectConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeMode {
    Skip,
    #[default]
    Accurate,
}

#[derive(Clone, Debug)]
pub struct ScanRequest {
    pub roots: Vec<PathBuf>,
    pub scope: ScanScope,
    pub age_filter: Option<Duration>,
    pub protection_policy: ProtectionPolicy,
    pub size_mode: SizeMode,
}

impl ScanRequest {
    pub fn validate(&self) -> crate::Result<()> {
        if self.roots.is_empty() {
            return Err(crate::ShatterError::InvalidRequest(
                "at least one root is required".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ScanItem {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub ecosystem: String,
    pub rule_name: String,
    pub bytes: Option<u64>,
    pub last_modified: Option<SystemTime>,
    pub project_root: Option<PathBuf>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ScanTotals {
    pub items: usize,
    pub bytes: u64,
    pub by_kind: BTreeMap<ArtifactKind, u64>,
}

#[derive(Clone, Debug)]
pub struct ScanWarning {
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ScanReport {
    pub items: Vec<ScanItem>,
    pub totals: ScanTotals,
    pub warnings: Vec<ScanWarning>,
    pub duration: Duration,
    pub cancelled: bool,
}

#[derive(Clone, Debug)]
pub enum ScanEvent {
    Started {
        roots: Vec<PathBuf>,
    },
    EnteredPath {
        path: PathBuf,
    },
    MatchFound {
        path: PathBuf,
        kind: ArtifactKind,
        rule_name: String,
    },
    Sized {
        path: PathBuf,
        bytes: u64,
    },
    Warning(ScanWarning),
    Finished {
        cancelled: bool,
        scanned_dirs: usize,
        matched_items: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteStrategy {
    #[default]
    Trash,
    Permanent,
}

impl Display for DeleteStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Trash => "trash",
            Self::Permanent => "permanent",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Debug)]
pub struct DeleteRequest {
    pub items: Vec<ScanItem>,
    pub strategy: DeleteStrategy,
}

#[derive(Clone, Debug)]
pub struct DeleteFailure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteResult {
    pub deleted: Vec<PathBuf>,
    pub failed: Vec<DeleteFailure>,
    pub reclaimed_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
