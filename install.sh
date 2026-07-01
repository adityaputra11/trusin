#!/usr/bin/env bash
set -euo pipefail

REPO="adityaputra11/terusin"
VERSION="${1:-latest}"
INSTALL_DIR="${TERUSIN_INSTALL:-/usr/local/bin}"

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo "Unsupported arch: $arch"; exit 1 ;;
  esac
  case "$os" in
    darwin) echo "${arch}-apple-darwin" ;;
    linux) echo "${arch}-unknown-linux-gnu" ;;
    *) echo "Unsupported OS: $os"; exit 1 ;;
  esac
}

main() {
  local platform
  platform="$(detect_platform)"

  if [ "$VERSION" = "latest" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
  fi

  local url="https://github.com/$REPO/releases/download/$VERSION/terusin-${platform}.tar.gz"
  echo " Downloading terusin $VERSION ($platform)..."

  local tmp
  tmp="$(mktemp -d)"
  curl -fsSL "$url" -o "$tmp/terusin.tar.gz"
  tar xzf "$tmp/terusin.tar.gz" -C "$tmp"

  mkdir -p "$INSTALL_DIR"
  mv "$tmp/terusin" "$INSTALL_DIR/terusin"
  chmod +x "$INSTALL_DIR/terusin"
  rm -rf "$tmp"

  echo " Installed terusin $VERSION to $INSTALL_DIR/terusin"
}

main
