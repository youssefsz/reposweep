use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use miette::IntoDiagnostic;
use reposweep_core::{
    ConfigService, DeleteRequest, DeleteService, DeleteStrategy, FileConfigStore,
    FsDeletionBackend, ProtectionPolicy, ScanRequest, ScanScope, ScanService, SizeMode,
    format_bytes,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "reposweep",
    about = "Clean developer repositories with a safer Rust-powered scanner."
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

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Tui { path: None }) {
        Command::Tui { path } => reposweep_tui::run(path).into_diagnostic()?,
        Command::Scan {
            path,
            scope,
            older_than,
            fast,
        } => run_scan(path, scope, older_than, fast).into_diagnostic()?,
        Command::Clean {
            path,
            scope,
            older_than,
            fast,
            strategy,
            yes,
        } => run_clean(path, scope, older_than, fast, strategy, yes).into_diagnostic()?,
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

fn run_scan(
    path: PathBuf,
    scope: ScopeArg,
    older_than: Option<String>,
    fast: bool,
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
