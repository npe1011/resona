#!/usr/bin/env bash
set -e

# ==============================================================================
# Resona macOS Build & App Bundle Script
# ==============================================================================

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${PROJECT_DIR}"

echo "==> Building Resona in release mode..."
cargo build --release

APP_NAME="Resona"
BUNDLE_DIR="${PROJECT_DIR}/target/bundle/osx/${APP_NAME}.app"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "==> Creating macOS App Bundle at: ${BUNDLE_DIR}"
rm -rf "${BUNDLE_DIR}"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

# 1. Copy executable binary
echo "==> Copying binary..."
cp "${PROJECT_DIR}/target/release/resona" "${MACOS_DIR}/resona"
chmod +x "${MACOS_DIR}/resona"

# 2. Copy Info.plist
echo "==> Copying Info.plist..."
cp "${PROJECT_DIR}/macos/Info.plist" "${CONTENTS_DIR}/Info.plist"

# 3. Create PkgInfo
echo "==> Creating PkgInfo..."
echo -n "APPL????" > "${CONTENTS_DIR}/PkgInfo"

# 4. Copy Icon
echo "==> Copying app icon..."
if [ -f "${PROJECT_DIR}/assets/icons/icon_mac.icns" ]; then
    cp "${PROJECT_DIR}/assets/icons/icon_mac.icns" "${RESOURCES_DIR}/icon.icns"
else
    echo "Warning: assets/icons/icon_mac.icns not found."
fi

# 5. Ad-hoc code signing (required on modern macOS / Apple Silicon)
if command -v codesign >/dev/null 2>&1; then
    echo "==> Applying ad-hoc code signature..."
    codesign --force --deep --sign - "${BUNDLE_DIR}"
fi

echo ""
echo "================================================================="
echo "  Build & Packaging Successful!"
echo "  Single binary: target/release/resona"
echo "  App bundle:    target/bundle/osx/${APP_NAME}.app"
echo "================================================================="
echo ""
echo "To test running directly from terminal:"
echo "  open \"${BUNDLE_DIR}\""
echo "  or: ./target/release/resona"
