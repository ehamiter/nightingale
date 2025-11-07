#!/bin/bash

# Build the release binary
echo "Building release binary..."
cargo build --release

# Fix yt-dlp shebang to use Python from PATH
YTDLP_PATH="$HOME/.local/bin/yt-dlp"
if [ -f "$YTDLP_PATH" ]; then
    echo "Fixing yt-dlp shebang to use Python from PATH..."
    PYTHON_PATH=$(which python3)
    if [ -n "$PYTHON_PATH" ]; then
        # Read the file, replace the shebang, and write it back
        tail -n +2 "$YTDLP_PATH" | cat <(echo "#!$PYTHON_PATH") - > "$YTDLP_PATH.tmp"
        mv "$YTDLP_PATH.tmp" "$YTDLP_PATH"
        chmod +x "$YTDLP_PATH"
        echo "✓ Updated shebang to: $PYTHON_PATH"
    fi
fi

# Create app bundle structure
APP_NAME="Nightingale"
APP_DIR="target/release/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "Creating app bundle structure..."
rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

# Copy the binary
echo "Copying binary..."
cp "target/release/nightingale" "${MACOS_DIR}/${APP_NAME}"

# Create Info.plist
echo "Creating Info.plist..."
cat > "${CONTENTS_DIR}/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.nightingale.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2025</string>
</dict>
</plist>
EOF

# Copy icon if it exists
if [ -f "assets/icon.icns" ]; then
    echo "Copying app icon..."
    cp assets/icon.icns "${RESOURCES_DIR}/"
else
    echo "⚠️  No icon found at assets/icon.icns"
    echo "   Run ./build_icon.sh with your icon image to add one"
fi

echo ""
echo "✅ App bundle created successfully!"
echo "📁 Location: ${APP_DIR}"
echo ""
echo "To run the app:"
echo "  open ${APP_DIR}"
echo ""
echo "To install to Applications folder:"
echo "  cp -r ${APP_DIR} /Applications/"
echo ""
