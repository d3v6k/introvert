#!/bin/bash
set -e

APP_NAME="Introvert"
DMG_NAME="Introvert-macOS"
BUILD_DIR="build/macos/Build/Products/Release"
APP_PATH="$BUILD_DIR/$APP_NAME.app"
DMG_PATH="$DMG_NAME.dmg"
VOLUME_NAME="$APP_NAME"

echo "🔨 Building macOS DMG for $APP_NAME..."

# Step 1: Build Rust native library
echo "🦀 Building Rust native library..."
cargo build --release
cp target/release/libintrovert.dylib .
cp target/release/libintrovert.dylib macos/Flutter/ephemeral/libintrovert.dylib

# Step 2: Build Flutter macOS app
echo "🦋 Building Flutter macOS app..."
flutter build macos --release

# Verify app exists
if [ ! -d "$APP_PATH" ]; then
    echo "❌ Error: $APP_PATH not found. Flutter build may have failed."
    exit 1
fi

# Step 3: Create DMG
echo "💿 Creating DMG..."

# Clean up any existing DMG
rm -f "$DMG_PATH"

# Create a temporary directory for DMG contents
TEMP_DIR=$(mktemp -d)
cp -R "$APP_PATH" "$TEMP_DIR/"

# Create symlink to Applications folder
ln -s /Applications "$TEMP_DIR/Applications"

# Create DMG using hdiutil
hdiutil create -volname "$VOLUME_NAME" \
    -srcfolder "$TEMP_DIR" \
    -ov -format UDZO \
    "$DMG_PATH"

# Clean up temp directory
rm -rf "$TEMP_DIR"

echo "✅ DMG created successfully: $DMG_PATH"
echo ""
echo "To install:"
echo "  1. Double-click $DMG_PATH"
echo "  2. Drag $APP_NAME to Applications"
echo "  3. Eject the disk image"
echo "  4. Launch from Applications or Spotlight"
