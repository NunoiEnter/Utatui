#!/bin/sh
# UtaTUI installer: downloads the latest static Linux binary from GitHub Releases.
#
#   curl -fsSL https://raw.githubusercontent.com/NunoiEnter/Utatui/main/install.sh | sh
#
# Environment variables:
#   UTATUI_VERSION      release tag to install (default: latest)
#   UTATUI_INSTALL_DIR  where to put the binary (default: ~/.local/bin)

set -eu

REPO="NunoiEnter/Utatui"
VERSION="${UTATUI_VERSION:-latest}"
INSTALL_DIR="${UTATUI_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '\033[1;35m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed"; }

[ "$(uname -s)" = "Linux" ] || die "UtaTUI only supports Linux (MPRIS is a Linux D-Bus interface)"

case "$(uname -m)" in
    x86_64 | amd64) target="x86_64-unknown-linux-musl" ;;
    aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
    *) die "no prebuilt binary for $(uname -m); install from source with: cargo install --git https://github.com/$REPO" ;;
esac

need tar
need sha256sum
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
    fetch_stdout() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
    fetch_stdout() { wget -qO- "$1"; }
else
    die "curl or wget is required"
fi

if [ "$VERSION" = "latest" ]; then
    VERSION=$(fetch_stdout "https://api.github.com/repos/$REPO/releases/latest" |
        sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
    [ -n "$VERSION" ] || die "could not determine the latest release"
fi

name="utatui-$VERSION-$target"
url="https://github.com/$REPO/releases/download/$VERSION/$name.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading UtaTUI $VERSION for $target"
fetch "$url" "$tmp/$name.tar.gz" || die "download failed: $url"
fetch "$url.sha256" "$tmp/$name.tar.gz.sha256" || die "checksum download failed"
(cd "$tmp" && sha256sum -c "$name.tar.gz.sha256" >/dev/null) || die "checksum mismatch, aborting"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/$name/utatui" "$INSTALL_DIR/utatui"
say "Installed $INSTALL_DIR/utatui"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) say "Add $INSTALL_DIR to your PATH, e.g.: echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile" ;;
esac

if ! command -v cava >/dev/null 2>&1; then
    hint="your package manager"
    if command -v pacman >/dev/null 2>&1; then hint="sudo pacman -S cava"
    elif command -v apt >/dev/null 2>&1; then hint="sudo apt install cava"
    elif command -v dnf >/dev/null 2>&1; then hint="sudo dnf install cava"
    elif command -v zypper >/dev/null 2>&1; then hint="sudo zypper install cava"
    elif command -v xbps-install >/dev/null 2>&1; then hint="sudo xbps-install cava"
    elif command -v apk >/dev/null 2>&1; then hint="sudo apk add cava"
    fi
    say "Optional: install cava for the audio visualizer ($hint)"
fi

say "Done. Start something playing and run: utatui"
