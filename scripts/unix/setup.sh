#!/bin/bash
# ============================================================
#  ReadMD macOS Setup - 100% Pure Rust Native Edition
#  - Build native binary + package macOS .app bundle
# ============================================================
set -e
cd "$(dirname "$0")/../.."

if [ "$(uname -s)" != "Darwin" ]; then
    echo "setup.sh builds a macOS .app and must run on macOS."
    echo "On Linux, use ./scripts/unix/install.sh then ./scripts/run.sh instead."
    exit 1
fi

echo "[1/4] Checking build environment ..."
if ! command -v cargo &>/dev/null; then
    echo "Rust toolchain (cargo) not found. Install the native Rust toolchain first."
    exit 1
fi

echo "[2/4] Compiling native release binary with Cargo ..."
cargo build --locked --offline --manifest-path rust/Cargo.toml --release -p readmd-kernel --bin readmd

echo "[3/4] Packaging ReadMD.app bundle ..."
APP_PATH="dist/ReadMD.app"
if [ -e "$APP_PATH" ]; then
    echo "Existing app bundle found at $APP_PATH; move it before creating a fresh candidate."
    exit 1
fi
mkdir -p "$APP_PATH/Contents/MacOS"
mkdir -p "$APP_PATH/Contents/Resources"

TARGET_PATH="${CARGO_TARGET_DIR:-rust/target}"
cp "$TARGET_PATH/release/readmd" "$APP_PATH/Contents/MacOS/ReadMD"
chmod +x "$APP_PATH/Contents/MacOS/ReadMD"

# Copy icons and resources if present
if [ -f "assets/ReadMD.icns" ]; then
    cp assets/ReadMD.icns "$APP_PATH/Contents/Resources/readmd.icns"
elif [ -f "assets/readmd.icns" ]; then
    cp assets/readmd.icns "$APP_PATH/Contents/Resources/readmd.icns"
fi
cp -R assets "$APP_PATH/Contents/Resources/"

APP_VERSION="$(tr -d '\r\n' < VERSION)"
MINIMUM_MACOS=10.15
if /usr/bin/lipo -archs "$APP_PATH/Contents/MacOS/ReadMD" | grep -qw arm64; then MINIMUM_MACOS=11.0; fi
cat <<PLIST > "$APP_PATH/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>ReadMD</string>
    <key>CFBundleDisplayName</key>
    <string>ReadMD</string>
    <key>CFBundleIdentifier</key>
    <string>asia.readmd.desktop</string>
    <key>CFBundleVersion</key>
    <string>${APP_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${APP_VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>ReadMD</string>
    <key>CFBundleIconFile</key>
    <string>readmd.icns</string>
    <key>LSMinimumSystemVersion</key>
    <string>${MINIMUM_MACOS}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array><dict>
        <key>CFBundleTypeName</key><string>Markdown</string>
        <key>CFBundleTypeRole</key><string>Editor</string>
        <key>LSHandlerRank</key><string>Alternate</string>
        <key>LSItemContentTypes</key><array><string>net.daringfireball.markdown</string></array>
        <key>CFBundleTypeExtensions</key><array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
    </dict></array>
    <key>UTImportedTypeDeclarations</key>
    <array><dict>
        <key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
        <key>UTTypeDescription</key><string>Markdown</string>
        <key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
        <key>UTTypeTagSpecification</key><dict>
            <key>public.filename-extension</key><array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
            <key>public.mime-type</key><string>text/markdown</string>
        </dict>
    </dict></array>
</dict>
</plist>
PLIST

READMD_MACOS_SIGNING_MODE=development node tools/macos-sign.mjs --path "$APP_PATH"
echo "[4/4] Done! Built: $APP_PATH"
echo "  Development signature only; this is not a notarized distribution."
echo
echo "  To run from source:  ./scripts/run.sh [file.md]"
echo "  To run packaged:     open \"$APP_PATH\""
echo "  To install system-wide: cp -r \"$APP_PATH\" /Applications/"
echo
