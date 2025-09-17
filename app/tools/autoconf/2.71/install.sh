#!/usr/bin/env bash
set -euo pipefail

# Idempotent installer for autoconf 2.69
# - downloads tarball if missing
# - extracts if source dir missing
# - configures, builds, and installs into ./install if not already present

VERSION=2.71
TARBALL="autoconf-${VERSION}.tar.gz"
SRC_DIR="autoconf-${VERSION}"
PREFIX="$(pwd)/install"
BIN="$PREFIX/bin/autoconf"

info() { echo "==> $*"; }
err() { echo "ERROR: $*" >&2; exit 1; }

if [ -x "$BIN" ]; then
	if "$BIN" --version >/dev/null 2>&1; then
		ver=$("$BIN" --version | head -n1 | awk '{print $NF}')
		if [ "$ver" = "$VERSION" ]; then
			info "autoconf $VERSION already installed at $PREFIX"
			exit 0
		else
			info "Different autoconf version ($ver) found at $BIN; will install $VERSION"
		fi
	fi
fi

if [ -f "$TARBALL" ]; then
	info "Tarball $TARBALL already exists, skipping download"
else
	info "Downloading $TARBALL..."
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL -o "$TARBALL" "https://ftp.gnu.org/gnu/autoconf/${TARBALL}" || err "download failed"
	elif command -v wget >/dev/null 2>&1; then
		wget -qO "$TARBALL" "https://ftp.gnu.org/gnu/autoconf/${TARBALL}" || err "download failed"
	else
		err "neither curl nor wget available to download $TARBALL"
	fi
fi

info "Preparing source"
# Always extract fresh source: remove existing source dir if present
if [ -d "$SRC_DIR" ]; then
	info "Removing existing source dir $SRC_DIR"
	rm -rf "$SRC_DIR"
fi
info "Extracting $TARBALL..."
tar xzf "$TARBALL"

cd "$SRC_DIR"

info "Configuring with prefix=$PREFIX"
./configure --prefix="$PREFIX"
MAKEJ="${MAKEJ:-$(nproc 2>/dev/null || echo 2)}"
info "Building with -j$MAKEJ"
make -j"$MAKEJ"
info "Installing"
make install

info "Installed autoconf at $BIN"
"$BIN" --version

# cleanup source dir after successful install
cd ..
info "Removing source dir $SRC_DIR"
rm -rf "$SRC_DIR"

exit 0