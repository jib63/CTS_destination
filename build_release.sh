#!/bin/bash
# Build a release binary for Debian/Linux and assemble a dist/ directory.
# Run this script on the Debian VM (or any Linux host with Rust installed).
#
# Usage:
#   chmod +x build_release.sh
#   ./build_release.sh
#
# To install Rust on the VM if not already present:
#   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
#   source "$HOME/.cargo/env"

set -eu

BINARY_NAME="cts-departures"
DIST_DIR="dist"

echo "==> Building release binary..."
cargo build --release

echo "==> Assembling $DIST_DIR/..."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

cp "target/release/$BINARY_NAME" "$DIST_DIR/"
cp config.toml "$DIST_DIR/"

# Force port 80 in the deployed config
sed -i 's|^\s*listen_addr\s*=.*|listen_addr = "0.0.0.0:80"|' "$DIST_DIR/config.toml"
echo "    Configured listen_addr = 0.0.0.0:80 in $DIST_DIR/config.toml"

# Copy token file if api_token_file is set in config
if grep -qE '^\s*api_token_file\s*=' config.toml 2>/dev/null; then
    TOKEN_FILE=$(grep -E '^\s*api_token_file\s*=' config.toml \
        | head -1 \
        | sed 's/.*=\s*["'"'"']\{0,1\}//; s/["'"'"']\s*$//')
    if [ -f "$TOKEN_FILE" ]; then
        cp "$TOKEN_FILE" "$DIST_DIR/"
        echo "    Copied token file: $TOKEN_FILE"
    else
        echo "    WARNING: api_token_file '$TOKEN_FILE' not found — copy it manually to $DIST_DIR/"
    fi
fi

SIZE=$(du -h "$DIST_DIR/$BINARY_NAME" | cut -f1)
echo ""
echo "==> Build complete!"
echo "    Binary : $DIST_DIR/$BINARY_NAME  ($SIZE)"
echo ""
echo "── Next steps ──────────────────────────────────────────────────────────"
echo ""
echo "  1. Allow binding port 80 without root (do once after deploy):"
echo "       sudo setcap cap_net_bind_service=+ep $DIST_DIR/$BINARY_NAME"
echo ""
echo "  2. Run directly:"
echo "       ./$DIST_DIR/$BINARY_NAME"
echo "     Or with an explicit config path:"
echo "       ./$DIST_DIR/$BINARY_NAME /path/to/config.toml"
echo ""
echo "  3. (Optional) Install as a systemd service so it starts on boot:"
echo "       sudo cp $DIST_DIR/$BINARY_NAME /usr/local/bin/"
echo "       sudo cp $DIST_DIR/config.toml  /etc/cts/"
echo "       sudo cp deploy/cts.service     /etc/systemd/system/"
echo "       sudo systemctl daemon-reload"
echo "       sudo systemctl enable --now cts"
echo ""

# ── Generate a ready-to-use systemd unit file ────────────────────────────────
mkdir -p deploy
cat > deploy/cts.service << 'EOF'
[Unit]
Description=CTS Departure Board
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=cts
ExecStart=/usr/local/bin/cts-departures /etc/cts/config.toml
Restart=on-failure
RestartSec=5
# Allow binding privileged ports (alternative to setcap)
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF

echo "  Systemd unit written to deploy/cts.service"
echo ""
echo "  To create the 'cts' system user:"
echo "       sudo useradd --system --no-create-home --shell /usr/sbin/nologin cts"
echo "       sudo mkdir -p /etc/cts && sudo chown cts:cts /etc/cts"
