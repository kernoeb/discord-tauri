#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${ROOT}/target/release"
APP_NAME="Discord Tauri"
APP_DIR="${TARGET_DIR}/${APP_NAME}.app"
BIN_NAME="discord-tauri"
VERSION="$(grep '^version' "${ROOT}/Cargo.toml" | head -1 | cut -d'"' -f2)"

if [[ ! -x "${TARGET_DIR}/${BIN_NAME}" ]]; then
  echo "binary not found at ${TARGET_DIR}/${BIN_NAME} — run cargo build --release first" >&2
  exit 1
fi

# Update the bundle in-place rather than rm -rf'ing it. macOS Sequoia (15+)
# tracks apps via Launch Services provenance (com.apple.provenance xattr);
# destroying and recreating the .app gives it a new identity, which makes TCC
# re-prompt for mic/camera on every rebuild even with a stable signing cert.
mkdir -p "${APP_DIR}/Contents/MacOS" "${APP_DIR}/Contents/Resources"

cp "${TARGET_DIR}/${BIN_NAME}" "${APP_DIR}/Contents/MacOS/${BIN_NAME}"
cp "${ROOT}/icons/icon.icns" "${APP_DIR}/Contents/Resources/icon.icns"

cat > "${APP_DIR}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleExecutable</key>
    <string>${BIN_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>discord.tauri</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>icon</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSCameraUsageDescription</key>
    <string>Discord needs camera access for video calls.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Discord needs microphone access for voice calls.</string>
</dict>
</plist>
EOF

# Sign with a stable local cert if available (so TCC permissions survive rebuilds).
# Override with SIGN_IDENTITY=... if you want a different cert.
SIGN_IDENTITY="${SIGN_IDENTITY:-discord-tauri local}"
if security find-certificate -c "${SIGN_IDENTITY}" >/dev/null 2>&1; then
  codesign --force --deep --sign "${SIGN_IDENTITY}" "${APP_DIR}"
  echo "signed with: ${SIGN_IDENTITY}"
else
  codesign --force --deep --sign - "${APP_DIR}" >/dev/null 2>&1 || true
  echo "ad-hoc signed (TCC permissions will reset on each rebuild)"
  echo "create a self-signed code signing cert named '${SIGN_IDENTITY}' in Keychain Access for stable permissions"
fi

echo "bundled: ${APP_DIR}"
