#!/usr/bin/env bash
set -euo pipefail
export COPYFILE_DISABLE=1

APP_NAME="Duplicate File Checker"
BINARY_NAME="find-dupl-file"
BUNDLE_ID="${BUNDLE_ID:-com.duplicatefilechecker.app}"
VERSION="${VERSION:-0.1.0}"
BUILD_DIR="target/macos-package-checker"
APP_DIR="$BUILD_DIR/$APP_NAME.app"
DMG_ROOT="$BUILD_DIR/dmg-root"
DMG_PATH="$BUILD_DIR/$APP_NAME-$VERSION.dmg"
ICON_SOURCE="assets/theme-toggle.png"
ICONSET_DIR="$BUILD_DIR/AppIcon.iconset"
ICNS_PATH="$APP_DIR/Contents/Resources/AppIcon.icns"

echo "Building release binary..."
cargo build --release

rm -rf "$BUILD_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources" "$DMG_ROOT"

echo "Creating app icon..."
mkdir -p "$ICONSET_DIR"
sips -z 16 16 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16.png" >/dev/null
sips -z 32 32 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32.png" >/dev/null
sips -z 64 64 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128.png" >/dev/null
sips -z 256 256 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256.png" >/dev/null
sips -z 512 512 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_512x512.png" >/dev/null
cp "$ICON_SOURCE" "$ICONSET_DIR/icon_512x512@2x.png"
iconutil -c icns "$ICONSET_DIR" -o "$ICNS_PATH"

echo "Creating app bundle..."
cp "target/release/$BINARY_NAME" "$APP_DIR/Contents/MacOS/$BINARY_NAME"
chmod +x "$APP_DIR/Contents/MacOS/$BINARY_NAME"

cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>zh_CN</string>
  <key>CFBundleExecutable</key>
  <string>$BINARY_NAME</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>重复文件清理器</string>
  <key>CFBundleDisplayName</key>
  <string>重复文件清理器</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

xattr -cr "$APP_DIR"

if [[ -n "${DEVELOPER_ID_APP:-}" ]]; then
  echo "Signing app with: $DEVELOPER_ID_APP"
  codesign --force --options runtime --timestamp --sign "$DEVELOPER_ID_APP" "$APP_DIR"
else
  echo "Signing app with ad-hoc identity..."
  codesign --force --deep --sign - "$APP_DIR"
fi
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

echo "Creating dmg..."
ditto --noextattr --noqtn "$APP_DIR" "$DMG_ROOT/$APP_NAME.app"
ln -s /Applications "$DMG_ROOT/Applications"
xattr -cr "$DMG_ROOT"
find "$DMG_ROOT" -name '._*' -delete
hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$DMG_ROOT" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null
hdiutil verify "$DMG_PATH" >/dev/null
rm -rf "$APP_DIR" "$DMG_ROOT" "$ICONSET_DIR"

echo
echo "Created:"
echo "  $DMG_PATH"
echo
echo "Validate locally with:"
echo "  hdiutil verify \"$DMG_PATH\""
