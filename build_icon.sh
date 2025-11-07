#!/bin/bash

# Icon generation script
# Usage: ./build_icon.sh path/to/your/icon.png

set -e

if [ $# -eq 0 ]; then
    echo "Usage: ./build_icon.sh path/to/your/icon.png"
    echo "Example: ./build_icon.sh my_icon.png"
    exit 1
fi

SOURCE_ICON="$1"

if [ ! -f "$SOURCE_ICON" ]; then
    echo "Error: Icon file not found: $SOURCE_ICON"
    exit 1
fi

echo "Creating assets directory..."
mkdir -p assets

echo "Processing icon for macOS (.icns)..."

# Create iconset directory
rm -rf icon.iconset
mkdir icon.iconset

# Generate all required sizes for macOS
sips -z 16 16     "$SOURCE_ICON" --out icon.iconset/icon_16x16.png
sips -z 32 32     "$SOURCE_ICON" --out icon.iconset/icon_16x16@2x.png
sips -z 32 32     "$SOURCE_ICON" --out icon.iconset/icon_32x32.png
sips -z 64 64     "$SOURCE_ICON" --out icon.iconset/icon_32x32@2x.png
sips -z 128 128   "$SOURCE_ICON" --out icon.iconset/icon_128x128.png
sips -z 256 256   "$SOURCE_ICON" --out icon.iconset/icon_128x128@2x.png
sips -z 256 256   "$SOURCE_ICON" --out icon.iconset/icon_256x256.png
sips -z 512 512   "$SOURCE_ICON" --out icon.iconset/icon_256x256@2x.png
sips -z 512 512   "$SOURCE_ICON" --out icon.iconset/icon_512x512.png
sips -z 1024 1024 "$SOURCE_ICON" --out icon.iconset/icon_512x512@2x.png

# Convert to .icns
iconutil -c icns icon.iconset -o assets/icon.icns

echo "Processing icon for Linux (.png)..."
# Create 512x512 PNG for Linux
sips -z 512 512 "$SOURCE_ICON" --out assets/nightingale.png

# Clean up
rm -rf icon.iconset

echo ""
echo "✅ Icons created successfully!"
echo "  macOS: assets/icon.icns"
echo "  Linux: assets/nightingale.png"
echo ""
echo "Next steps:"
echo "  1. Run ./build_app.sh (macOS) or ./build_linux.sh (Linux)"
echo "  2. The icon will be automatically included"
