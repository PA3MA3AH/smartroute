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

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║    SmartRoute Uninstall Script        ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root${NC}"
    echo "Please run: sudo $0"
    exit 1
fi

# Stop and disable service
echo -e "${YELLOW}[1/5]${NC} Stopping SmartRoute service..."
if systemctl is-active --quiet smartroute; then
    systemctl stop smartroute
    echo -e "${GREEN}✓ Service stopped${NC}"
fi

if systemctl is-enabled --quiet smartroute 2>/dev/null; then
    systemctl disable smartroute
    echo -e "${GREEN}✓ Service disabled${NC}"
fi

# Remove systemd files
echo -e "${YELLOW}[2/5]${NC} Removing systemd files..."
rm -f "$SYSTEMD_DIR/smartroute.service"
rm -f "$SYSTEMD_DIR/smartroute@.service"
systemctl daemon-reload
echo -e "${GREEN}✓ Systemd files removed${NC}"

# Remove binary
echo -e "${YELLOW}[3/5]${NC} Removing binary..."
rm -f "$INSTALL_DIR/smartroute"
echo -e "${GREEN}✓ Binary removed${NC}"

# Ask about config
echo -e "${YELLOW}[4/5]${NC} Configuration files..."
read -p "Remove configuration directory $CONFIG_DIR? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf "$CONFIG_DIR"
    echo -e "${GREEN}✓ Configuration removed${NC}"
else
    echo -e "${BLUE}ℹ Configuration kept at $CONFIG_DIR${NC}"
fi

# Clean runtime files
echo -e "${YELLOW}[5/5]${NC} Cleaning runtime files..."
rm -rf /run/smartroute
rm -rf /var/log/smartroute
echo -e "${GREEN}✓ Runtime files cleaned${NC}"

echo ""
echo -e "${GREEN}╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  SmartRoute uninstalled successfully!  ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════╝${NC}"
