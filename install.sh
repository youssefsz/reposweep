#!/usr/bin/env bash

set -euo pipefail

REPO="${REPOSWEEP_REPO:-youssefsz/reposweep}"
BINARY_NAME="reposweep"
INSTALL_DIR="${REPOSWEEP_INSTALL_DIR:-$HOME/.local/bin}"
REQUESTED_VERSION="${REPOSWEEP_VERSION:-latest}"
USE_SOURCE_INSTALL=0

usage() {
  cat <<'EOF'
Install RepoSweep from GitHub Releases.

Usage:
  curl -sSL https://raw.githubusercontent.com/youssefsz/reposweep/main/install.sh | bash
  powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/youssefsz/reposweep/main/install.ps1 | iex"

Options:
  --version <tag>     Install a specific release tag, for example v0.1.0
  --dir <path>        Install into a custom directory
  --from-source       Build with cargo instead of downloading a release
  --help              Show this help

Environment overrides:
  REPOSWEEP_REPO        GitHub repo slug, for example youssefsz/reposweep
  REPOSWEEP_VERSION     Release tag or "latest"
  REPOSWEEP_INSTALL_DIR Destination directory, defaults to ~/.local/bin
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      REQUESTED_VERSION="$2"
      shift 2
      ;;
    --dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --from-source)
      USE_SOURCE_INSTALL=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)
      echo "Unsupported operating system for install.sh: $os" >&2
      echo "On Windows, use install.ps1 instead." >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *)
      echo "Unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  printf '%s-%s' "$arch" "$os"
}

resolve_version() {
  if [[ "$REQUESTED_VERSION" != "latest" ]]; then
    printf '%s' "$REQUESTED_VERSION"
    return
  fi

  need_cmd curl
  local version
  version="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  if [[ -z "$version" ]]; then
    echo "Failed to resolve the latest release tag for $REPO" >&2
    exit 1
  fi
  printf '%s' "$version"
}

install_from_source() {
  need_cmd cargo
  echo "Building RepoSweep from source into $INSTALL_DIR"
  cargo install \
    --locked \
    --git "https://github.com/$REPO.git" \
    --bin "$BINARY_NAME" \
    --root "${REPOSWEEP_CARGO_ROOT:-$HOME/.cargo}" \
    reposweep

  local cargo_bin
  cargo_bin="${REPOSWEEP_CARGO_ROOT:-$HOME/.cargo}/bin/$BINARY_NAME"
  mkdir -p "$INSTALL_DIR"
  cp "$cargo_bin" "$INSTALL_DIR/$BINARY_NAME"
  chmod +x "$INSTALL_DIR/$BINARY_NAME"
}

install_from_release() {
  need_cmd curl
  need_cmd tar

  local version target archive url temp_dir
  target="$(detect_target)"
  version="$(resolve_version)"
  archive="${BINARY_NAME}-${version}-${target}.tar.gz"
  url="https://github.com/$REPO/releases/download/$version/$archive"
  temp_dir="$(mktemp -d)"

  cleanup() {
    rm -rf "$temp_dir"
  }
  trap cleanup EXIT

  echo "Downloading $archive"
  curl -fL "$url" -o "$temp_dir/$archive"

  mkdir -p "$INSTALL_DIR"
  tar -xzf "$temp_dir/$archive" -C "$temp_dir"

  if [[ ! -f "$temp_dir/$BINARY_NAME" ]]; then
    echo "Archive did not contain $BINARY_NAME" >&2
    exit 1
  fi

  install -m 755 "$temp_dir/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
}

main() {
  if [[ "$USE_SOURCE_INSTALL" -eq 1 ]]; then
    install_from_source
  else
    install_from_release
  fi

  echo
  echo "Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      echo "Add this to your shell profile if needed:"
      echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac
}

main "$@"
