#!/bin/bash
# Build and package cts-departures v1.4.0 for the Freebox Delta.
#
# This script cross-compiles the binary, generates a version-specific
# install.sh (with config migration from v1.3.0 → v1.4.0), and bundles
# everything into a single zip for deployment.
#
# Run on macOS M1/M2/M3 (same prerequisites as build_freebox.sh).
#
# Usage:
#   ./build_freeboxV1.4.sh
#
# Output:
#   cts-departures-v1.4.0-freebox.zip
#
# Deployment (on the Freebox):
#   scp cts-departures-v1.4.0-freebox.zip jib@freebox:~/
#   ssh jib@freebox
#   unzip cts-departures-v1.4.0-freebox.zip
#   cd cts-departures-v1.4.0
#   bash install.sh
#
# What install.sh does on the Freebox:
#   • Copies the binary to /home/jib/cts-jaures, cts-gallia, cts-portehop
#   • Migrates each config.toml:
#       - Removes pixoo64_delay_between_screens (obsolete)
#       - Adds the three new Ambient Hub keys as comments
#   • For cts-gallia and cts-portehop: ensures pixoo64 stays disabled

set -eu

APP_VERSION="1.4.0"
TARGET="aarch64-unknown-linux-musl"
BINARY_NAME="cts-departures"
ZIG_VER="0.13.0"
ZIG_DIR="$HOME/.local/zig-${ZIG_VER}"

PACKAGE_DIR="cts-departures-v${APP_VERSION}"
ZIP_NAME="cts-departures-v${APP_VERSION}-freebox.zip"

# ── 1. Ensure Zig is available ────────────────────────────────────────────────
if ! command -v zig &>/dev/null; then
    if [ -f "$ZIG_DIR/zig" ]; then
        export PATH="$ZIG_DIR:$PATH"
    else
        echo "==> Downloading Zig ${ZIG_VER} (aarch64-macos)..."
        mkdir -p "$ZIG_DIR"
        curl -fL \
            "https://ziglang.org/download/${ZIG_VER}/zig-macos-aarch64-${ZIG_VER}.tar.xz" \
            -o /tmp/zig.tar.xz
        tar -xf /tmp/zig.tar.xz --strip-components=1 -C "$ZIG_DIR"
        rm /tmp/zig.tar.xz
        export PATH="$ZIG_DIR:$PATH"
        echo "    Zig installed to $ZIG_DIR"
    fi
fi
echo "==> Using Zig $(zig version)"

# ── 2. Ensure the Rust target is available ────────────────────────────────────
if ! rustup target list --installed | grep -q "$TARGET"; then
    echo "==> Adding Rust target $TARGET..."
    rustup target add "$TARGET"
fi

# ── 3. Ensure cargo-zigbuild is installed ─────────────────────────────────────
if ! cargo zigbuild --version &>/dev/null 2>&1; then
    echo "==> Installing cargo-zigbuild..."
    cargo install cargo-zigbuild
fi

# ── 4. Cross-compile ──────────────────────────────────────────────────────────
echo "==> Cross-compiling v${APP_VERSION} for $TARGET..."
cargo zigbuild --target "$TARGET" --release

BINARY_PATH="target/$TARGET/release/$BINARY_NAME"
echo "==> Build complete: $BINARY_PATH"
file "$BINARY_PATH"
ls -lh "$BINARY_PATH"

# ── 5. Assemble package directory ─────────────────────────────────────────────
echo ""
echo "==> Assembling ${PACKAGE_DIR}/..."
rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR"

cp "$BINARY_PATH" "$PACKAGE_DIR/"
chmod +x "$PACKAGE_DIR/$BINARY_NAME"

# ── 6. Generate install.sh ────────────────────────────────────────────────────
echo "==> Generating install.sh..."

cat > "$PACKAGE_DIR/install.sh" <<'INSTALL_EOF'
#!/bin/bash
# install.sh — Deploy cts-departures v1.4.0 on the Freebox Delta
#
# Run this script on the Freebox after unzipping the package:
#   bash install.sh
#
# What this script does:
#   1. Copies cts-departures to each instance directory
#   2. Migrates each config.toml (v1.3.0 → v1.4.0):
#        - Removes pixoo64_delay_between_screens (replaced by three separate keys)
#        - Inserts the three new Ambient Hub config keys as comments
#   3. Ensures pixoo64 remains disabled for cts-gallia and cts-portehop
#
# A .v1.3.bak backup of each config.toml is created before any change.

set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="$SCRIPT_DIR/cts-departures"

INSTANCES=(
    "/home/jib/cts-jaures"
    "/home/jib/cts-gallia"
    "/home/jib/cts-portehop"
)

# These instances must not have pixoo64 enabled
PIXOO_OFF_INSTANCES=(
    "/home/jib/cts-gallia"
    "/home/jib/cts-portehop"
)

# ── Helpers ───────────────────────────────────────────────────────────────────

is_pixoo_off() {
    local path="$1"
    for off in "${PIXOO_OFF_INSTANCES[@]}"; do
        [ "$off" = "$path" ] && return 0
    done
    return 1
}

migrate_config() {
    local cfg="$1"
    local disable_pixoo="$2"

    if [ ! -f "$cfg" ]; then
        echo "    [warn] config.toml not found at $cfg — skipping migration"
        return
    fi

    # Backup
    cp "$cfg" "${cfg}.v1.3.bak"
    echo "    Backed up to $(basename "${cfg}.v1.3.bak")"

    # ── Remove obsolete key ──────────────────────────────────────────────────
    # Removes both the active and any commented-out form of the old key.
    sed -i -E '/^[[:space:]]*(#[[:space:]]*)?pixoo64_delay_between_screens[[:space:]]*=/d' "$cfg"
    echo "    Removed pixoo64_delay_between_screens"

    # ── Insert new Ambient Hub keys after the last pixoo64_* line ────────────
    # Only if they are not already present (idempotent).
    if grep -qE '^[[:space:]]*(#[[:space:]]*)?pixoo64_tram_screen_seconds' "$cfg" 2>/dev/null; then
        echo "    New Ambient Hub keys already present — skipping insertion"
    else
        # Find the last line that starts with any pixoo64_ key (active or commented)
        # and append the three new keys immediately after it.
        awk '
            /^[[:space:]]*(#[[:space:]]*)?pixoo64_/ { last = NR }
            { lines[NR] = $0 }
            END {
                for (i = 1; i <= NR; i++) {
                    print lines[i]
                    if (i == last) {
                        print "# pixoo64_tram_screen_seconds   = 6   # seconds each stop screen is shown (1..60)"
                        print "# pixoo64_moment_screen_seconds = 1   # seconds each moment screen is shown (1..30)"
                        print "# pixoo64_lines_per_screen      = 4   # departure rows per screen (1..4)"
                    }
                }
            }
        ' "$cfg" > "${cfg}.tmp" && mv "${cfg}.tmp" "$cfg"
        echo "    Inserted pixoo64_tram_screen_seconds, pixoo64_moment_screen_seconds, pixoo64_lines_per_screen"
    fi

    # ── Enforce pixoo64 disabled for gallia / portehop ───────────────────────
    if [ "$disable_pixoo" = "true" ]; then
        # If an uncommented pixoo64_enabled = true line exists, comment it out.
        if grep -qE '^[[:space:]]*pixoo64_enabled[[:space:]]*=[[:space:]]*true' "$cfg"; then
            sed -i -E \
                's|^([[:space:]]*)pixoo64_enabled([[:space:]]*)=([[:space:]]*)true|\1# pixoo64_enabled = false  # kept off for this instance|' \
                "$cfg"
            echo "    [safety] pixoo64_enabled forced off (was true)"
        else
            echo "    pixoo64_enabled already off — OK"
        fi
    fi
}

# ── Main loop ─────────────────────────────────────────────────────────────────

echo "==> Installing cts-departures v1.4.0"
echo ""

for INST in "${INSTANCES[@]}"; do
    echo "── $INST"

    if [ ! -d "$INST" ]; then
        echo "   [skip] directory not found"
        echo ""
        continue
    fi

    # Copy binary
    cp "$BINARY" "$INST/cts-departures"
    chmod +x "$INST/cts-departures"
    echo "    Binary installed"

    # Migrate config
    if is_pixoo_off "$INST"; then
        migrate_config "$INST/config.toml" "true"
    else
        migrate_config "$INST/config.toml" "false"
    fi

    echo ""
done

echo "==> All instances updated."
echo ""
echo "    Restart services to apply:"
echo "      sudo systemctl restart cts-jaures cts-gallia cts-portehop"
echo ""
INSTALL_EOF

chmod +x "$PACKAGE_DIR/install.sh"
echo "    install.sh generated"

# ── 7. Create zip ─────────────────────────────────────────────────────────────
echo ""
echo "==> Creating $ZIP_NAME..."
rm -f "$ZIP_NAME"
zip -r "$ZIP_NAME" "$PACKAGE_DIR/"
echo "    $(du -h "$ZIP_NAME" | cut -f1)  $ZIP_NAME"

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "==> Package ready: $ZIP_NAME"
echo ""
echo "    Transfer and install on the Freebox:"
echo "      scp $ZIP_NAME jib@freebox:~/"
echo "      ssh jib@freebox"
echo "      unzip $ZIP_NAME"
echo "      cd $PACKAGE_DIR"
echo "      bash install.sh"
echo ""
