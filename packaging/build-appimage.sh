#!/usr/bin/env bash
# Build a samaku AppImage using linuxdeploy.
#
# Prerequisites (must already be installed on the build host):
#   libass (with shared library), ffms2 (with shared library), GOMP,
#   SuiteSparse, OpenBLAS  — same deps required to compile samaku.
#
# The script downloads linuxdeploy and its GTK plugin on first run and
# caches them next to this script so subsequent runs are offline-capable.
#
# Usage:
#   cd <repo-root>
#   bash packaging/build-appimage.sh
#
# Output: samaku-x86_64.AppImage (or the appropriate ARCH suffix) in the
# repo root.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

ARCH="${ARCH:-$(uname -m)}"

LINUXDEPLOY="${SCRIPT_DIR}/linuxdeploy-${ARCH}.AppImage"
LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${ARCH}.AppImage"

APPIMAGETOOL="${SCRIPT_DIR}/appimagetool-${ARCH}.AppImage"
APPIMAGETOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${ARCH}.AppImage"

# ---------------------------------------------------------------------------
# 1. Fetch tooling if not already cached
# ---------------------------------------------------------------------------
if [[ ! -x "${LINUXDEPLOY}" ]]; then
    echo "Downloading linuxdeploy for ${ARCH}…"
    curl -fsSL -o "${LINUXDEPLOY}" "${LINUXDEPLOY_URL}"
    chmod +x "${LINUXDEPLOY}"
fi

if [[ ! -x "${APPIMAGETOOL}" ]]; then
    echo "Downloading appimagetool for ${ARCH}…"
    curl -fsSL -o "${APPIMAGETOOL}" "${APPIMAGETOOL_URL}"
    chmod +x "${APPIMAGETOOL}"
fi

# ---------------------------------------------------------------------------
# 2. Build the release binary
# ---------------------------------------------------------------------------
echo "Building samaku (release)…"
cd "${REPO_ROOT}"
cargo build --release

BINARY="${REPO_ROOT}/target/release/samaku"

# ---------------------------------------------------------------------------
# 3. Assemble the AppDir
# ---------------------------------------------------------------------------
APPDIR="${REPO_ROOT}/packaging/AppDir"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

cp "${BINARY}" "${APPDIR}/usr/bin/samaku"
cp "${SCRIPT_DIR}/samaku.desktop" "${APPDIR}/usr/share/applications/samaku.desktop"
cp "${REPO_ROOT}/src/resources/logo.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/samaku.svg"

# ---------------------------------------------------------------------------
# 4. Run linuxdeploy to deploy shared libraries into the AppDir
#
# libleancrypto is excluded here because linuxdeploy's bundled patchelf
# (0.15.0) corrupts its IFUNC resolver tables, causing a segfault at
# startup. It is copied in unmodified afterwards (step 5) so the AppImage
# remains fully self-contained.
# ---------------------------------------------------------------------------
echo "Running linuxdeploy…"
cd "${REPO_ROOT}"
mkdir -p "${REPO_ROOT}/target/appimage"

ARCH="${ARCH}" \
NO_STRIP=1 \
"${LINUXDEPLOY}" --appimage-extract-and-run \
    --appdir "${APPDIR}" \
    --desktop-file "${APPDIR}/usr/share/applications/samaku.desktop" \
    --icon-file "${APPDIR}/usr/share/icons/hicolor/scalable/apps/samaku.svg" \
    --exclude-library "libleancrypto.so.1"
# no --output appimage: we call appimagetool ourselves after step 5

# ---------------------------------------------------------------------------
# 5. Bundle libleancrypto unmodified (bypassing patchelf)
# ---------------------------------------------------------------------------
LIBLEANCRYPTO="$(ldconfig -p | grep 'libleancrypto\.so\.1' | awk '{print $NF}')"
if [[ -z "${LIBLEANCRYPTO}" ]]; then
    echo "ERROR: libleancrypto.so.1 not found on this system." >&2
    exit 1
fi
cp "${LIBLEANCRYPTO}" "${APPDIR}/usr/lib/libleancrypto.so.1"

# Aliases for OpenBLAS
ln -sf libopenblas.so.0 "${APPDIR}/usr/lib/liblapack.so.3"
ln -sf libopenblas.so.0 "${APPDIR}/usr/lib/libblas.so.3"

# ---------------------------------------------------------------------------
# 6. Package the AppDir into an AppImage
# ---------------------------------------------------------------------------
echo "Packaging AppImage…"
OUTPUT="${REPO_ROOT}/target/appimage/samaku-${ARCH}.AppImage"
ARCH="${ARCH}" "${APPIMAGETOOL}" --appimage-extract-and-run "${APPDIR}" "${OUTPUT}"

echo ""
echo "Done: ${OUTPUT}"
