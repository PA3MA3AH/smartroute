#!/bin/bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/smartroute"
SYSTEMD_DIR="/etc/systemd/system"
REPO="PA3MA3AH/smartroute"

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     SmartRoute Installation Script    ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root${NC}"
    echo "Please run: sudo $0"
    exit 1
fi

# Check dependencies
echo -e "${YELLOW}[1/6]${NC} Checking dependencies..."
if ! command -v curl &> /dev/null && ! command -v wget &> /dev/null; then
    echo -e "${RED}Error: curl or wget is required${NC}"
    exit 1
fi

if ! command -v sing-box &> /dev/null; then
    echo -e "${YELLOW}Warning: sing-box not found. SmartRoute requires sing-box to work.${NC}"
    echo "Install it from: https://sing-box.sagernet.org/"
fi

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
        exit 1
        ;;
esac

# Get latest release
echo -e "${YELLOW}[2/6]${NC} Fetching latest release..."
if command -v curl &> /dev/null; then
    LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
else
    LATEST_RELEASE=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
fi

if [ -z "$LATEST_RELEASE" ]; then
    echo -e "${RED}Error: Could not fetch latest release${NC}"
    exit 1
fi

echo -e "${GREEN}Latest version: $LATEST_RELEASE${NC}"

# Download binary
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/smartroute-$LATEST_RELEASE-$TARGET.tar.gz"
TMP_DIR=$(mktemp -d)
TMP_FILE="$TMP_DIR/smartroute.tar.gz"

echo -e "${YELLOW}[3/6]${NC} Downloading SmartRoute..."
if command -v curl &> /dev/null; then
    curl -L -o "$TMP_FILE" "$DOWNLOAD_URL"
else
    wget -O "$TMP_FILE" "$DOWNLOAD_URL"
fi

# Extract and install
echo -e "${YELLOW}[4/6]${NC} Installing binary..."
tar -xzf "$TMP_FILE" -C "$TMP_DIR"
mv "$TMP_DIR/smartroute" "$INSTALL_DIR/smartroute"
chmod +x "$INSTALL_DIR/smartroute"
rm -rf "$TMP_DIR"

echo -e "${GREEN}✓ Binary installed to $INSTALL_DIR/smartroute${NC}"

# Create config directory
echo -e "${YELLOW}[5/6]${NC} Setting up configuration..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    cat > "$CONFIG_DIR/config.toml" <<EOF
[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "direct"

# Add your nodes here
# Example:
# [[nodes]]
# tag = "my-proxy"
# type = "vless"
# server = "example.com"
# port = 443
# uuid = "your-uuid"
# security = "reality"
# server_name = "example.com"
EOF
    echo -e "${GREEN}✓ Created default config at $CONFIG_DIR/config.toml${NC}"
else
    echo -e "${BLUE}ℹ Config already exists at $CONFIG_DIR/config.toml${NC}"
fi

# Install systemd service
echo -e "${YELLOW}[6/6]${NC} Installing systemd service..."
if [ -d "$SYSTEMD_DIR" ]; then
    # Download systemd files from repo
    SYSTEMD_URL="https://raw.githubusercontent.com/$REPO/master/systemd"

    if command -v curl &> /dev/null; then
        curl -s -o "$SYSTEMD_DIR/smartroute.service" "$SYSTEMD_URL/smartroute.service"
        curl -s -o "$SYSTEMD_DIR/smartroute@.service" "$SYSTEMD_URL/smartroute@.service"
    else
        wget -q -O "$SYSTEMD_DIR/smartroute.service" "$SYSTEMD_URL/smartroute.service"
        wget -q -O "$SYSTEMD_DIR/smartroute@.service" "$SYSTEMD_URL/smartroute@.service"
    fi

    systemctl daemon-reload
    echo -e "${GREEN}✓ Systemd service installed${NC}"
else
    echo -e "${YELLOW}⚠ Systemd not found, skipping service installation${NC}"
fi

# Create runtime directory
mkdir -p /run/smartroute
mkdir -p /var/log/smartroute

echo ""
echo -e "${GREEN}╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║   SmartRoute installed successfully!   ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo "  1. Edit config: sudo nano $CONFIG_DIR/config.toml"
echo "  2. Start service: sudo systemctl start smartroute"
echo "  3. Enable autostart: sudo systemctl enable smartroute"
echo "  4. Check status: sudo systemctl status smartroute"
echo ""
echo -e "${BLUE}Or run manually:${NC}"
echo "  sudo smartroute start $CONFIG_DIR/config.toml"
echo ""
echo -e "${BLUE}Documentation:${NC} https://github.com/$REPO"
