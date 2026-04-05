use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{Context, IntoDiagnostic, Result, miette};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderValue, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use tempfile::tempdir;
use walkdir::WalkDir;

const DEFAULT_REPO: &str = "youssefsz/reposweep";
const BINARY_NAME: &str = if cfg!(windows) {
    "reposweep.exe"
} else {
    "reposweep"
};

#[derive(Debug, Clone)]
pub struct UpgradeOptions {
    pub requested_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

impl ArchiveKind {
    fn extension(self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Zip => "zip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlatformTarget {
    triple: &'static str,
    archive_kind: ArchiveKind,
}

impl PlatformTarget {
    fn detect() -> Result<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Ok(Self {
                triple: "x86_64-unknown-linux-gnu",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("linux", "aarch64") => Ok(Self {
                triple: "aarch64-unknown-linux-gnu",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("macos", "x86_64") => Ok(Self {
                triple: "x86_64-apple-darwin",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("macos", "aarch64") => Ok(Self {
                triple: "aarch64-apple-darwin",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("windows", "x86_64") => Ok(Self {
                triple: "x86_64-pc-windows-msvc",
                archive_kind: ArchiveKind::Zip,
            }),
            (os, arch) => Err(miette!(
                "`reposweep upgrade` is not available for {arch} on {os} yet."
            )),
        }
    }

    fn asset_name(self, version_tag: &str) -> String {
        format!(
            "reposweep-{version_tag}-{}.{}",
            self.triple,
            self.archive_kind.extension()
        )
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub fn run(options: UpgradeOptions) -> Result<()> {
    let repo = std::env::var("REPOSWEEP_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    let target = PlatformTarget::detect()?;
    let current_exe = std::env::current_exe().into_diagnostic()?;
    let current_version = Version::parse(env!("CARGO_PKG_VERSION")).into_diagnostic()?;
    let requested_tag = normalize_requested_tag(options.requested_version.as_deref());

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .into_diagnostic()?;

    let spinner = new_spinner("Checking for a fresh RepoSweep build...")?;
    let release = fetch_release(&client, &repo, requested_tag.as_deref())?;
    let release_version = parse_release_version(&release.tag_name)?;

    if release_version == current_version {
        spinner.finish_with_message(format!(
            "Already on {}. RepoSweep checked twice and found nothing newer.",
            release.tag_name
        ));
        return Ok(());
    }

    if requested_tag.is_none() && release_version < current_version {
        spinner.finish_with_message(format!(
            "Already ahead on v{current_version}. RepoSweep politely refused to downgrade itself."
        ));
        return Ok(());
    }

    let asset_name = target.asset_name(&release.tag_name);
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| missing_asset_error(&release, target, &asset_name))?;

    spinner.finish_and_clear();

    let temp_dir = tempdir().into_diagnostic()?;
    let archive_path = temp_dir.path().join(&asset.name);
    download_release_asset(&client, asset, &archive_path)?;

    let unpack_spinner = new_spinner("Unpacking the new binary...")?;
    let extracted_binary = extract_release_binary(&archive_path, temp_dir.path(), target)?;
    unpack_spinner.finish_and_clear();

    let install_spinner = new_spinner("Swapping binaries safely...")?;
    self_replace::self_replace(&extracted_binary)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to replace {}", current_exe.display()))?;
    install_spinner.finish_with_message(format!(
        "Upgrade complete: v{current_version} -> {}. RepoSweep changed outfits without making a mess.",
        release.tag_name
    ));

    println!("Updated {}", current_exe.display());
    Ok(())
}

fn fetch_release(
    client: &Client,
    repo: &str,
    requested_tag: Option<&str>,
) -> Result<GitHubRelease> {
    let url = match requested_tag {
        Some(tag) => format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };

    client
        .get(url)
        .header(USER_AGENT, github_user_agent()?)
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .into_diagnostic()?
        .error_for_status()
        .into_diagnostic()
        .wrap_err_with(|| match requested_tag {
            Some(tag) => format!("Could not find release {tag} in {repo}"),
            None => format!("Could not resolve the latest release in {repo}"),
        })?
        .json::<GitHubRelease>()
        .into_diagnostic()
        .wrap_err("GitHub returned an unexpected release payload")
}

fn download_release_asset(client: &Client, asset: &GitHubAsset, destination: &Path) -> Result<()> {
    let mut response = client
        .get(&asset.browser_download_url)
        .header(USER_AGENT, github_user_agent()?)
        .send()
        .into_diagnostic()?
        .error_for_status()
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to download {}", asset.name))?;

    let total_bytes = response.content_length().unwrap_or(asset.size);
    let progress = if total_bytes > 0 {
        let bar = ProgressBar::new(total_bytes);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} {msg}\n[{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .into_diagnostic()?
            .progress_chars("=> "),
        );
        bar
    } else {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg} {bytes}").into_diagnostic()?,
        );
        spinner.enable_steady_tick(Duration::from_millis(100));
        spinner
    };
    progress.set_message(format!("Downloading {}", asset.name));

    let mut output = File::create(destination).into_diagnostic()?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = response.read(&mut buffer).into_diagnostic()?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).into_diagnostic()?;
        progress.inc(read as u64);
    }

    progress.finish_with_message(format!("Downloaded {}", asset.name));
    Ok(())
}

fn extract_release_binary(
    archive_path: &Path,
    workspace: &Path,
    target: PlatformTarget,
) -> Result<PathBuf> {
    let extract_dir = workspace.join("release");
    fs::create_dir_all(&extract_dir).into_diagnostic()?;

    match target.archive_kind {
        ArchiveKind::TarGz => extract_tar_archive(archive_path, &extract_dir)?,
        ArchiveKind::Zip => extract_zip_archive(archive_path, &extract_dir)?,
    }

    find_binary(&extract_dir)
}

fn extract_tar_archive(archive_path: &Path, extract_dir: &Path) -> Result<()> {
    let file = File::open(archive_path).into_diagnostic()?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().into_diagnostic()? {
        let mut entry = entry.into_diagnostic()?;
        let unpacked = entry.unpack_in(extract_dir).into_diagnostic()?;
        if !unpacked {
            return Err(miette!(
                "Release archive contained a path that could not be unpacked safely."
            ));
        }
    }

    Ok(())
}

fn extract_zip_archive(archive_path: &Path, extract_dir: &Path) -> Result<()> {
    let file = File::open(archive_path).into_diagnostic()?;
    let mut archive = zip::ZipArchive::new(file).into_diagnostic()?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).into_diagnostic()?;
        let Some(relative_path) = entry.enclosed_name().map(PathBuf::from) else {
            continue;
        };
        let output_path = extract_dir.join(relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&output_path).into_diagnostic()?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }

        let mut output = File::create(&output_path).into_diagnostic()?;
        std::io::copy(&mut entry, &mut output).into_diagnostic()?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode))
                .into_diagnostic()?;
        }
    }

    Ok(())
}

fn find_binary(root: &Path) -> Result<PathBuf> {
    let direct_path = root.join(BINARY_NAME);
    if direct_path.is_file() {
        return Ok(direct_path);
    }

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file() && entry.file_name() == OsStr::new(BINARY_NAME) {
            return Ok(entry.into_path());
        }
    }

    Err(miette!(
        "The downloaded archive did not include the {BINARY_NAME} binary."
    ))
}

fn parse_release_version(tag_name: &str) -> Result<Version> {
    let normalized = tag_name.trim().trim_start_matches('v');
    Version::parse(normalized)
        .into_diagnostic()
        .wrap_err_with(|| format!("Release tag {tag_name} is not a valid semantic version"))
}

fn normalize_requested_tag(input: Option<&str>) -> Option<String> {
    let trimmed = input?.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("latest") {
        return None;
    }

    Some(if trimmed.starts_with('v') {
        trimmed.to_string()
    } else {
        format!("v{trimmed}")
    })
}

fn missing_asset_error(
    release: &GitHubRelease,
    target: PlatformTarget,
    expected_name: &str,
) -> miette::Report {
    let available_assets = release
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect::<Vec<_>>();

    miette!(
        "Release {} does not include {} for {}. Available assets: {}",
        release.tag_name,
        expected_name,
        target.triple,
        available_assets.join(", ")
    )
}

fn new_spinner(message: impl Into<String>) -> Result<ProgressBar> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .into_diagnostic()?
            .tick_strings(&["-", "\\", "|", "/"]),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner.set_message(message.into());
    Ok(spinner)
}

fn github_user_agent() -> Result<HeaderValue> {
    HeaderValue::from_str(&format!("reposweep/{}", env!("CARGO_PKG_VERSION")))
        .into_diagnostic()
        .wrap_err("Failed to build GitHub API user agent")
}

#[cfg(test)]
mod tests {
    use super::{ArchiveKind, PlatformTarget, normalize_requested_tag, parse_release_version};

    #[test]
    fn normalize_requested_tag_adds_v_prefix() {
        assert_eq!(
            normalize_requested_tag(Some("0.3.0")),
            Some("v0.3.0".into())
        );
        assert_eq!(
            normalize_requested_tag(Some("v0.3.0")),
            Some("v0.3.0".into())
        );
        assert_eq!(normalize_requested_tag(Some("latest")), None);
        assert_eq!(normalize_requested_tag(Some("  ")), None);
    }

    #[test]
    fn parse_release_version_accepts_v_prefix() {
        let version = parse_release_version("v1.2.3").expect("version should parse");
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn asset_name_matches_release_convention() {
        let linux = PlatformTarget {
            triple: "x86_64-unknown-linux-gnu",
            archive_kind: ArchiveKind::TarGz,
        };
        let windows = PlatformTarget {
            triple: "x86_64-pc-windows-msvc",
            archive_kind: ArchiveKind::Zip,
        };

        assert_eq!(
            linux.asset_name("v0.4.0"),
            "reposweep-v0.4.0-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            windows.asset_name("v0.4.0"),
            "reposweep-v0.4.0-x86_64-pc-windows-msvc.zip"
        );
    }
}
