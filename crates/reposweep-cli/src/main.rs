mod completions;
mod doctor;
mod uninstall;
mod upgrade;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use miette::IntoDiagnostic;
use reposweep_core::{
    ConfigService, DeleteRequest, DeleteService, DeleteStrategy, FileConfigStore,
    FsDeletionBackend, ProtectionPolicy, ScanRequest, ScanScope, ScanService, SizeMode,
    format_bytes,
};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "reposweep",
    about = "Clean developer repositories with a safer Rust-powered scanner.",
    version,
    author = "Youssef Dhibi <https://dhibi.tn>",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Tui {
        path: Option<PathBuf>,
    },
    Scan {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = ScopeArg::All)]
        scope: ScopeArg,
        #[arg(long)]
        older_than: Option<String>,
        #[arg(long)]
        fast: bool,
        #[arg(
            long,
            help = "Emit machine-readable JSON instead of human-readable text"
        )]
        json: bool,
    },
    Clean {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = ScopeArg::All)]
        scope: ScopeArg,
        #[arg(long)]
        older_than: Option<String>,
        #[arg(long)]
        fast: bool,
        #[arg(long, value_enum, default_value_t = StrategyArg::Trash)]
        strategy: StrategyArg,
        #[arg(long)]
        yes: bool,
    },
    /// Upgrade the installed RepoSweep binary in place.
    #[command(disable_version_flag = true)]
    Upgrade {
        #[arg(
            long,
            value_name = "TAG",
            help = "Install a specific release tag instead of the latest one"
        )]
        version: Option<String>,
    },
    /// Remove the installed RepoSweep binary from this machine.
    Uninstall {
        #[arg(long, help = "Skip the confirmation prompt")]
        yes: bool,
    },
    /// Inspect the current RepoSweep installation and configuration paths.
    Doctor,
    /// Generate shell completions to stdout.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Init,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ScopeArg {
    Cache,
    Dependencies,
    All,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum StrategyArg {
    Trash,
    Permanent,
}

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .init();

    if std::env::args().any(|arg| arg == "--youssef") {
        show_youssef_easter_egg();
        return Ok(());
    }

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Tui { path: None }) {
        Command::Tui { path } => reposweep_tui::run(path).into_diagnostic()?,
        Command::Scan {
            path,
            scope,
            older_than,
            fast,
            json,
        } => run_scan(path, scope, older_than, fast, json).into_diagnostic()?,
        Command::Clean {
            path,
            scope,
            older_than,
            fast,
            strategy,
            yes,
        } => run_clean(path, scope, older_than, fast, strategy, yes).into_diagnostic()?,
        Command::Upgrade { version } => upgrade::run(upgrade::UpgradeOptions {
            requested_version: version,
        })?,
        Command::Uninstall { yes } => uninstall::run(uninstall::UninstallOptions { yes })?,
        Command::Doctor => doctor::run()?,
        Command::Completions { shell } => completions::run(shell)?,
        Command::Config { command } => match command {
            ConfigCommand::Init => {
                let service = ConfigService::new(FileConfigStore);
                let paths = service.save_default().into_diagnostic()?;
                println!("Wrote default config to {}", paths.config_file.display());
            }
        },
    }
    Ok(())
}

fn show_youssef_easter_egg() {
    let art = [
        "RRRRR   EEEEE  PPPP    OOO    SSSS  W   W  EEEEE  EEEEE  PPPP",
        "R   RR  E      P   P  O   O  S      W   W  E      E      P   P",
        "RRRRR   EEEE   PPPP   O   O   SSS   W W W  EEEE   EEEE   PPPP",
        "R  RR   E      P      O   O      S  WW WW  E      E      P",
        "R   RR  EEEEE  P       OOO   SSSS   W   W  EEEEE  EEEEE  P",
        "",
        "Sweep smart. Ship clean.",
        "Created by Youssef Dhibi",
        "https://dhibi.tn",
    ];

    println!();
    for line in art {
        println!("{line}");
    }
    println!();
}

fn run_scan(
    path: PathBuf,
    scope: ScopeArg,
    older_than: Option<String>,
    fast: bool,
    json: bool,
) -> reposweep_core::Result<()> {
    let report = build_service().scan(
        ScanRequest {
            roots: vec![path],
            scope: scope.into(),
            age_filter: parse_age_filter(older_than.as_deref())?,
            protection_policy: ProtectionPolicy::RespectConfig,
            size_mode: if fast {
                SizeMode::Skip
            } else {
                SizeMode::Accurate
            },
        },
        None,
        reposweep_core::CancellationToken::new(),
    )?;

    if json {
        let payload = JsonScanReport::from_report(report);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| {
                reposweep_core::RepoSweepError::InvalidRequest(format!(
                    "failed to serialize scan report as JSON: {error}"
                ))
            })?
        );
        return Ok(());
    }

    println!(
        "Found {} items reclaiming {} in {:?}",
        report.totals.items,
        format_bytes(report.totals.bytes),
        report.duration
    );
    for item in report.items {
        println!(
            "{:>12}  {:<12}  {}",
            item.bytes.map(format_bytes).unwrap_or_else(|| "n/a".into()),
            item.kind,
            item.path.display()
        );
    }
    if !report.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in report.warnings {
            if let Some(path) = warning.path {
                println!("- {}: {}", path.display(), warning.message);
            } else {
                println!("- {}", warning.message);
            }
        }
    }
    Ok(())
}

fn run_clean(
    path: PathBuf,
    scope: ScopeArg,
    older_than: Option<String>,
    fast: bool,
    strategy: StrategyArg,
    yes: bool,
) -> reposweep_core::Result<()> {
    if !yes {
        return Err(reposweep_core::RepoSweepError::InvalidRequest(
            "pass --yes to confirm non-interactive cleanup".into(),
        ));
    }

    let report = build_service().scan(
        ScanRequest {
            roots: vec![path],
            scope: scope.into(),
            age_filter: parse_age_filter(older_than.as_deref())?,
            protection_policy: ProtectionPolicy::RespectConfig,
            size_mode: if fast {
                SizeMode::Skip
            } else {
                SizeMode::Accurate
            },
        },
        None,
        reposweep_core::CancellationToken::new(),
    )?;
    let result = DeleteService::new(FsDeletionBackend).delete(DeleteRequest {
        items: report.items,
        strategy: strategy.into(),
    });

    println!(
        "Deleted {} items, failed on {}, reclaimed {}",
        result.deleted.len(),
        result.failed.len(),
        format_bytes(result.reclaimed_bytes)
    );
    for failure in result.failed {
        println!("- {}: {}", failure.path.display(), failure.message);
    }
    Ok(())
}

fn build_service() -> ScanService {
    let config = ConfigService::new(FileConfigStore).load_or_default();
    ScanService::from_config(config)
}

#[derive(Debug, Serialize)]
struct JsonScanReport {
    items: Vec<JsonScanItem>,
    totals: JsonScanTotals,
    warnings: Vec<JsonScanWarning>,
    duration_ms: u128,
    cancelled: bool,
}

impl JsonScanReport {
    fn from_report(report: reposweep_core::ScanReport) -> Self {
        Self {
            items: report.items.into_iter().map(JsonScanItem::from).collect(),
            totals: JsonScanTotals::from(report.totals),
            warnings: report
                .warnings
                .into_iter()
                .map(JsonScanWarning::from)
                .collect(),
            duration_ms: report.duration.as_millis(),
            cancelled: report.cancelled,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonScanItem {
    path: String,
    kind: reposweep_core::ArtifactKind,
    ecosystem: String,
    rule_name: String,
    bytes: Option<u64>,
    last_modified_unix_ms: Option<u128>,
    project_root: Option<String>,
    notes: Vec<String>,
}

impl From<reposweep_core::ScanItem> for JsonScanItem {
    fn from(item: reposweep_core::ScanItem) -> Self {
        Self {
            path: item.path.display().to_string(),
            kind: item.kind,
            ecosystem: item.ecosystem,
            rule_name: item.rule_name,
            bytes: item.bytes,
            last_modified_unix_ms: item.last_modified.and_then(system_time_to_unix_millis),
            project_root: item.project_root.map(|path| path.display().to_string()),
            notes: item.notes,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonScanTotals {
    items: usize,
    bytes: u64,
    by_kind: JsonScanTotalsByKind,
}

impl From<reposweep_core::ScanTotals> for JsonScanTotals {
    fn from(totals: reposweep_core::ScanTotals) -> Self {
        Self {
            items: totals.items,
            bytes: totals.bytes,
            by_kind: JsonScanTotalsByKind {
                cache: totals
                    .by_kind
                    .get(&reposweep_core::ArtifactKind::Cache)
                    .copied()
                    .unwrap_or(0),
                dependency: totals
                    .by_kind
                    .get(&reposweep_core::ArtifactKind::Dependency)
                    .copied()
                    .unwrap_or(0),
                build: totals
                    .by_kind
                    .get(&reposweep_core::ArtifactKind::Build)
                    .copied()
                    .unwrap_or(0),
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonScanTotalsByKind {
    cache: u64,
    dependency: u64,
    build: u64,
}

#[derive(Debug, Serialize)]
struct JsonScanWarning {
    path: Option<String>,
    message: String,
}

impl From<reposweep_core::ScanWarning> for JsonScanWarning {
    fn from(warning: reposweep_core::ScanWarning) -> Self {
        Self {
            path: warning.path.map(|path| path.display().to_string()),
            message: warning.message,
        }
    }
}

fn system_time_to_unix_millis(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn parse_age_filter(input: Option<&str>) -> reposweep_core::Result<Option<Duration>> {
    let Some(input) = input else {
        return Ok(None);
    };

    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }

    let split_at = input
        .find(|char: char| !char.is_ascii_digit())
        .unwrap_or(input.len());
    let (digits, unit) = input.split_at(split_at);
    let value: u64 = digits.parse().map_err(|_| {
        reposweep_core::RepoSweepError::InvalidRequest(format!(
            "invalid --older-than value: {input}"
        ))
    })?;

    let seconds = match unit {
        "h" => value * 60 * 60,
        "d" => value * 60 * 60 * 24,
        "w" => value * 60 * 60 * 24 * 7,
        "m" => value * 60 * 60 * 24 * 30,
        "y" => value * 60 * 60 * 24 * 365,
        "" => value * 60 * 60 * 24,
        _ => {
            return Err(reposweep_core::RepoSweepError::InvalidRequest(format!(
                "unsupported duration unit in --older-than: {input}"
            )));
        }
    };
    Ok(Some(Duration::from_secs(seconds)))
}

impl From<ScopeArg> for ScanScope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Cache => ScanScope::Cache,
            ScopeArg::Dependencies => ScanScope::Dependencies,
            ScopeArg::All => ScanScope::All,
        }
    }
}

impl From<StrategyArg> for DeleteStrategy {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::Trash => DeleteStrategy::Trash,
            StrategyArg::Permanent => DeleteStrategy::Permanent,
        }
    }
}
