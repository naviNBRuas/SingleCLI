#!/bin/sh
# SingleCLI installer.
#
#   curl -fsSL https://raw.githubusercontent.com/naviNBRuas/SingleCLI/main/install.sh | sh
#
# Downloads the prebuilt `single` (CLI/TUI) and `single-runtimed` (runtime
# daemon) binaries for your platform from the latest GitHub release and
# installs them to $SINGLE_INSTALL_DIR (default: ~/.local/bin).
#
# Supported platforms: Linux x86_64/arm64, macOS x86_64/arm64 (Intel and
# Apple Silicon). Anything else: build from source with
# `cargo build --release --workspace` instead (see README.md).

set -eu

REPO="naviNBRuas/SingleCLI"
INSTALL_DIR="${SINGLE_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${SINGLE_VERSION:-latest}"

info() { printf '>> %s\n' "$1"; }
error() { printf 'error: %s\n' "$1" >&2; exit 1; }

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os_name="linux" ;;
    Darwin) os_name="macos" ;;
    *) error "unsupported OS: $os (build from source instead, see README.md)" ;;
  esac

  case "$arch" in
    x86_64 | amd64) arch_name="x86_64" ;;
    arm64 | aarch64) arch_name="arm64" ;;
    *) error "unsupported architecture: $arch (build from source instead, see README.md)" ;;
  esac

  printf '%s-%s' "$os_name" "$arch_name"
}

main() {
  command -v curl >/dev/null 2>&1 || error "curl is required"
  command -v tar >/dev/null 2>&1 || error "tar is required"

  target="$(detect_target)"
  info "Detected platform: $target"

  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/$REPO/releases/latest/download/singlecli-$target.tar.gz"
  else
    url="https://github.com/$REPO/releases/download/$VERSION/singlecli-$target.tar.gz"
  fi

  work_dir="$(mktemp -d)"
  trap 'rm -rf "$work_dir"' EXIT

  info "Downloading $url"
  if ! curl -fsSL "$url" -o "$work_dir/singlecli.tar.gz"; then
    error "download failed — is there a release for $target yet? See https://github.com/$REPO/releases"
  fi

  tar -xzf "$work_dir/singlecli.tar.gz" -C "$work_dir"

  mkdir -p "$INSTALL_DIR"
  extracted_dir="$work_dir/singlecli-$target"
  cp "$extracted_dir/single" "$INSTALL_DIR/single"
  cp "$extracted_dir/single-runtimed" "$INSTALL_DIR/single-runtimed"
  chmod +x "$INSTALL_DIR/single" "$INSTALL_DIR/single-runtimed"

  info "Installed to $INSTALL_DIR/single and $INSTALL_DIR/single-runtimed"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      info "$INSTALL_DIR is not on your PATH. Add it with:"
      printf '\n'
      printf '  bash/zsh:  echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc   # or ~/.zshrc\n' "$INSTALL_DIR"
      printf '  fish:      fish_add_path %s\n' "$INSTALL_DIR"
      printf '\n'
      ;;
  esac

  info "Run 'single doctor' to check what SingleCLI can manage on this machine, or just 'single' to open the TUI."
}

main
