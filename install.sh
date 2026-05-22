#!/usr/bin/env sh
# keybr-tui installer — downloads the latest prebuilt release binary from GitHub
# and installs it into /usr/local/bin (if writable) or ~/.local/bin.
#
# Usage:
#   curl -sSf https://y0sif.github.io/keybr-tui/install.sh | sh
#   curl -sSf https://raw.githubusercontent.com/y0sif/keybr-tui/main/install.sh | sh
#   ./install.sh
#
# Environment:
#   KEYBR_TUI_VERSION=v0.1.0   Pin to a specific tag (default: latest)

set -eu

REPO="y0sif/keybr-tui"
BIN="keybr-tui"

# ---- Detect platform ---------------------------------------------------------
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64|amd64) TARGET="x86_64-unknown-linux-gnu" ;;
            *)
                echo "Unsupported Linux architecture: $ARCH" >&2
                echo "Please build from source: https://github.com/$REPO" >&2
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64) TARGET="x86_64-apple-darwin" ;;
            arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
            *)
                echo "Unsupported macOS architecture: $ARCH" >&2
                exit 1
                ;;
        esac
        ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
        echo "Windows is not supported by this installer." >&2
        echo "Download the Windows release manually from:" >&2
        echo "  https://github.com/$REPO/releases/latest" >&2
        exit 1
        ;;
    *)
        echo "Unsupported operating system: $OS" >&2
        exit 1
        ;;
esac

# ---- Required tools ----------------------------------------------------------
for cmd in curl tar mktemp uname sed; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Required command not found: $cmd" >&2
        exit 1
    fi
done

# ---- Resolve release tag -----------------------------------------------------
if [ -n "${KEYBR_TUI_VERSION:-}" ]; then
    TAG="$KEYBR_TUI_VERSION"
    echo "Pinned release: $TAG"
else
    echo "Resolving latest release of $REPO..."
    # Follow the /releases/latest redirect to the canonical tag URL, then
    # extract the trailing tag name. No GitHub API token or jq required.
    TAG=$(curl -sSL -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest" \
        | sed 's|.*/tag/||' | tr -d '[:space:]')

    if [ -z "${TAG:-}" ] || [ "$TAG" = "latest" ]; then
        echo "Could not resolve the latest release tag." >&2
        echo "Set KEYBR_TUI_VERSION=v0.1.0 (or similar) and re-run." >&2
        exit 1
    fi
    echo "Latest release: $TAG"
fi

# ---- Download and extract ----------------------------------------------------
# Asset name matches release.yml: keybr-tui-<target>.tar.gz (unversioned).
ASSET="$BIN-$TARGET.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT INT HUP TERM

echo "Downloading $URL"
if ! curl -fL "$URL" -o "$TMPDIR/$ASSET"; then
    echo "Download failed." >&2
    echo "Check that release $TAG has artifact $ASSET:" >&2
    echo "  https://github.com/$REPO/releases/tag/$TAG" >&2
    exit 1
fi

echo "Extracting..."
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

# Locate the binary within the extracted tree.
BIN_PATH=$(find "$TMPDIR" -type f -name "$BIN" -print | head -n1)
if [ -z "${BIN_PATH:-}" ] || [ ! -f "$BIN_PATH" ]; then
    echo "Could not find '$BIN' binary in the downloaded archive." >&2
    exit 1
fi

chmod +x "$BIN_PATH"

# ---- Choose install destination ----------------------------------------------
SYSTEM_DIR="/usr/local/bin"
USER_DIR="$HOME/.local/bin"

if [ "$(id -u)" = "0" ]; then
    INSTALL_DIR="$SYSTEM_DIR"
elif [ -w "$SYSTEM_DIR" ]; then
    INSTALL_DIR="$SYSTEM_DIR"
else
    INSTALL_DIR="$USER_DIR"
    mkdir -p "$INSTALL_DIR"
fi

DEST="$INSTALL_DIR/$BIN"
mv "$BIN_PATH" "$DEST"
chmod +x "$DEST"

# ---- Done --------------------------------------------------------------------
echo ""
echo "Installed $BIN $TAG to $DEST"

case ":${PATH:-}:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo ""
        echo "Note: $INSTALL_DIR is not in your PATH."
        echo "Add this to your shell profile (~/.profile, ~/.bashrc, ~/.zshrc, or ~/.config/fish/config.fish):"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac

echo "Run '$BIN --help' to get started."
