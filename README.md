# Shatter

Shatter is a Rust-powered cleanup tool for developer repositories. It scans a chosen project path, detects disposable build outputs, caches, and dependency folders, then lets you review and remove them through a terminal UI or scriptable CLI.

## Install

Install the latest release on Linux or macOS:

```bash
curl -sSL https://raw.githubusercontent.com/youssefsz/shatter-rust/main/install.sh | bash
```

Install the latest release on Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/youssefsz/shatter-rust/main/install.ps1 | iex"
```

Install a specific version:

```bash
curl -sSL https://raw.githubusercontent.com/youssefsz/shatter-rust/main/install.sh | bash -s -- --version v0.1.0
```

```powershell
powershell -ExecutionPolicy Bypass -c "$env:SHATTER_VERSION='v0.1.0'; irm https://raw.githubusercontent.com/youssefsz/shatter-rust/main/install.ps1 | iex"
```

Build from source:

```bash
cargo install --path crates/shatter-cli
```

## Usage

Launch the terminal UI:

```bash
shatter
```

Scan a path from the CLI:

```bash
shatter scan ~/projects/my-app
```

Clean a path non-interactively:

```bash
shatter clean ~/projects/my-app --yes
```

Initialize the default config:

```bash
shatter config init
```

## Release Assets

The install script expects GitHub Release archives named like:

- `shatter-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `shatter-v0.1.0-x86_64-apple-darwin.tar.gz`
- `shatter-v0.1.0-aarch64-apple-darwin.tar.gz`
- `shatter-v0.1.0-x86_64-pc-windows-msvc.zip`

Those archives are produced automatically by the release workflow when you push a `v*` tag.
