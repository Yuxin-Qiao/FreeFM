#!/bin/sh
set -eu

REPO="Yuxin-Qiao/FreeFM"
INSTALL_DIR="${FREEFM_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${FREEFM_VERSION:-latest}"

os_type=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$os_type" in
  darwin) os="darwin" ;;
  linux) os="linux" ;;
  *)
    echo "Error: FreeFM installer currently supports macOS (darwin) and Linux." >&2
    echo "Windows support is planned for a future release." >&2
    exit 1
    ;;
esac

arch_type=$(uname -m)
case "$arch_type" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="arm64" ;;
  *)
    echo "Error: Unsupported architecture: $arch_type (FreeFM supports x86_64 and arm64)." >&2
    exit 1
    ;;
esac

if [ "$VERSION" = "latest" ]; then
  TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
  if [ -z "$TAG" ]; then
    # Fallback tag for repository release artifact url
    TAG="v0.1.0"
  fi
else
  TAG="$VERSION"
fi

ARTIFACT="freefm-${TAG}-${os}-${arch}.tar.gz"
BASE_URL="${FREEFM_BASE_URL:-https://github.com/${REPO}/releases/download/${TAG}}"
DOWNLOAD_URL="${BASE_URL}/${ARTIFACT}"
CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/freefm-install.XXXXXX")
trap 'rm -rf "$tmpdir"' EXIT HUP INT TERM

echo "Installing FreeFM (${TAG}) for ${os}-${arch}..."

if ! curl -fsSL "$DOWNLOAD_URL" -o "$tmpdir/$ARTIFACT"; then
  echo "Error: Failed to download artifact from $DOWNLOAD_URL" >&2
  echo "Please verify the tag '${TAG}' and network connectivity." >&2
  exit 1
fi

echo "Downloading SHA-256 checksum..."
if ! curl -fsSL "$CHECKSUM_URL" -o "$tmpdir/${ARTIFACT}.sha256"; then
  echo "Error: Failed to download checksum file from $CHECKSUM_URL" >&2
  echo "Refusing to install unverified artifact." >&2
  exit 1
fi

expected_sha=$(awk '{print $1}' "$tmpdir/${ARTIFACT}.sha256" | tr -d '\r\n')
if [ -z "$expected_sha" ]; then
  echo "Error: Checksum file $CHECKSUM_URL is empty or invalid." >&2
  exit 1
fi

echo "Verifying SHA-256 checksum..."
if command -v sha256sum >/dev/null 2>&1; then
  actual_sha=$(sha256sum "$tmpdir/$ARTIFACT" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  actual_sha=$(shasum -a 256 "$tmpdir/$ARTIFACT" | awk '{print $1}')
else
  echo "Error: Neither sha256sum nor shasum is available to verify artifact checksum." >&2
  echo "Refusing to install unverified artifact." >&2
  exit 1
fi

if [ "$expected_sha" != "$actual_sha" ]; then
  echo "Error: Checksum mismatch for $ARTIFACT!" >&2
  echo "Expected: $expected_sha" >&2
  echo "Actual:   $actual_sha" >&2
  echo "Refusing to install corrupted or tampered artifact." >&2
  exit 1
fi
echo "Checksum verified cleanly."

mkdir -p "$tmpdir/extracted"
tar -C "$tmpdir/extracted" -xzf "$tmpdir/$ARTIFACT"

found_bin=""
if [ -f "$tmpdir/extracted/freefm" ]; then
  found_bin="$tmpdir/extracted/freefm"
else
  found_bin=$(find "$tmpdir/extracted" -type f -name freefm | head -n 1)
fi

if [ -z "$found_bin" ] || [ ! -f "$found_bin" ]; then
  echo "Error: freefm binary not found inside $ARTIFACT" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$found_bin" "$INSTALL_DIR/freefm"
chmod 755 "$INSTALL_DIR/freefm"

echo "Verifying installation..."
"$INSTALL_DIR/freefm" --version

echo ""
echo "FreeFM installed successfully to $INSTALL_DIR/freefm"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Notice: $INSTALL_DIR is not currently in your PATH."
    echo "You can add it to your PATH by adding this line to your shell profile (~/.zshrc or ~/.bashrc):"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    ;;
esac
