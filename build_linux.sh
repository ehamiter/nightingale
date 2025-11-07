#!/bin/bash

# Build script for Linux standalone application
# Creates a .desktop file and installs the app to ~/.local/share/applications

set -e

echo "Building release binary..."
cargo build --release

echo "Creating Linux app structure..."

# Create directories
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/512x512/apps
mkdir -p ~/.local/bin

# Copy binary to local bin
cp target/release/nightingale ~/.local/bin/nightingale
chmod +x ~/.local/bin/nightingale

# Create .desktop file
cat > ~/.local/share/applications/nightingale.desktop << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Nightingale
Comment=YouTube Music Search and Downloader
Exec=nightingale
Icon=nightingale
Terminal=false
Categories=AudioVideo;Audio;Player;
Keywords=youtube;music;download;mp3;
StartupNotify=true
EOF

# Make desktop file executable
chmod +x ~/.local/share/applications/nightingale.desktop

# Copy icon if it exists
if [ -f "assets/nightingale.png" ]; then
    echo "Copying app icon..."
    cp assets/nightingale.png ~/.local/share/icons/hicolor/512x512/apps/
    # Update icon cache
    if command -v gtk-update-icon-cache &> /dev/null; then
        gtk-update-icon-cache ~/.local/share/icons/hicolor/ -f 2>/dev/null || true
    fi
else
    echo "⚠️  No icon found at assets/nightingale.png"
    echo "   Run ./build_icon.sh with your icon image to add one"
fi

echo ""
echo "✅ Linux app installed successfully!"
echo ""
echo "The app has been installed to:"
echo "  Binary: ~/.local/bin/nightingale"
echo "  Desktop file: ~/.local/share/applications/nightingale.desktop"
echo ""
echo "To run the app:"
echo "  1. Search for 'Nightingale' in your application launcher"
echo "  2. Or run: nightingale"
echo ""
echo "Note: Make sure ~/.local/bin is in your PATH"
echo "Add to ~/.bashrc or ~/.zshrc if needed:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "To add an icon, place a 512x512 PNG image at:"
echo "  ~/.local/share/icons/hicolor/512x512/apps/nightingale.png"
