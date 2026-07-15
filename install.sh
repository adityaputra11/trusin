#!/bin/sh
set -eu

REPO="adityaputra11/terusin"
VERSION="${TERUSIN_VERSION:-${1:-latest}}"
INSTALL_DIR="${TERUSIN_INSTALL:-/usr/local/bin}"

fail() {
  echo "error: $*" >&2
  exit 1
}

detect_platform() {
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) fail "unsupported architecture: $arch" ;;
  esac

  case "$os" in
    darwin) echo "${arch}-apple-darwin" ;;
    linux) echo "${arch}-unknown-linux-gnu" ;;
    *) fail "unsupported operating system: $os" ;;
  esac
}

release_base_url() {
  if [ "$VERSION" = "latest" ]; then
    echo "https://github.com/$REPO/releases/latest/download"
  else
    echo "https://github.com/$REPO/releases/download/$VERSION"
  fi
}

verify_checksum() {
  archive="$1"
  checksums="$2"
  asset="$3"
  expected="$(awk -v file="$asset" '$2 == file { print $1 }' "$checksums")"
  [ -n "$expected" ] || fail "checksum for $asset was not found in SHA256SUMS"

  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive" | awk '{ print $1 }')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
  else
    fail "sha256sum or shasum is required to verify the download"
  fi

  [ "$expected" = "$actual" ] || fail "checksum verification failed for $asset"
}

install_binary() {
  binary="$1"
  destination="$INSTALL_DIR/terusin"

  if mkdir -p "$INSTALL_DIR" 2>/dev/null && install -m 0755 "$binary" "$destination" 2>/dev/null; then
    return
  fi

  command -v sudo >/dev/null 2>&1 || fail "cannot write to $INSTALL_DIR; set TERUSIN_INSTALL to a writable directory"
  echo "Installing to $destination requires sudo."
  sudo mkdir -p "$INSTALL_DIR"
  sudo install -m 0755 "$binary" "$destination"
}

main() {
  platform="$(detect_platform)"
  asset="terusin-${platform}.tar.gz"
  base_url="$(release_base_url)"
  temp_dir="$(mktemp -d)"
  trap 'rm -rf "$temp_dir"' EXIT

  echo "Downloading terusin ${VERSION} (${platform})..."
  curl --fail --location --silent --show-error "$base_url/$asset" -o "$temp_dir/$asset"
  curl --fail --location --silent --show-error "$base_url/SHA256SUMS" -o "$temp_dir/SHA256SUMS"
  verify_checksum "$temp_dir/$asset" "$temp_dir/SHA256SUMS" "$asset"

  tar -xzf "$temp_dir/$asset" -C "$temp_dir"
  [ -f "$temp_dir/terusin" ] || fail "release archive does not contain the terusin binary"
  install_binary "$temp_dir/terusin"

  echo "Installed terusin ${VERSION} to ${INSTALL_DIR}/terusin"
  echo "Next: terusin set-token ts_your_token"
}

main
