#!/bin/bash
# Cross-compile cts-departures for the Freebox Delta (aarch64 Linux, musl libc).
#
# Run this on your macOS M1/M2/M3 machine — no Docker needed.
#
# Prerequisites (one-time setup):
#   cargo install cargo-zigbuild
#   rustup target add aarch64-unknown-linux-musl
#
# The Zig cross-linker is downloaded automatically by this script if not
# already installed at ~/.local/zig-0.13.0.
#
# Usage:
#   chmod +x build_freebox.sh
#   ./build_freebox.sh
#
# The resulting binary is a fully static ELF (no glibc dependency) — just
# copy it to the Freebox and run it.

set -eu

TARGET="aarch64-unknown-linux-musl"
BINARY_NAME="cts-departures"
ZIG_VER="0.13.0"
ZIG_DIR="$HOME/.local/zig-${ZIG_VER}"
DIST_DIR="dist-freebox"

# ── Remote deploy target ──────────────────────────────────────────────────────
# Override with:  REMOTE=myuser@192.168.1.1 ./build_freebox.sh
REMOTE="${REMOTE:-user@freebox}"
REMOTE_INSTANCES=(cts-gallia cts-jaures cts-portehop)

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
echo "==> Cross-compiling for $TARGET..."
cargo zigbuild --target "$TARGET" --release

BINARY_PATH="target/$TARGET/release/$BINARY_NAME"
echo "==> Build complete: $BINARY_PATH"
file "$BINARY_PATH"
ls -lh "$BINARY_PATH"

# ── 5. Assemble dist-freebox/ ─────────────────────────────────────────────────
echo ""
echo "==> Assembling $DIST_DIR/..."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

cp "$BINARY_PATH"   "$DIST_DIR/"
cp config.toml      "$DIST_DIR/"

# Force port 80 in the deployed config
sed -i.bak 's|^\s*listen_addr\s*=.*|listen_addr = "0.0.0.0:80"|' "$DIST_DIR/config.toml"
rm -f "$DIST_DIR/config.toml.bak"
echo "    Configured listen_addr = 0.0.0.0:80 in $DIST_DIR/config.toml"

# Copy token file if api_token_file is set in config
if grep -qE '^\s*api_token_file\s*=' config.toml 2>/dev/null; then
    TOKEN_FILE=$(grep -E '^\s*api_token_file\s*=' config.toml \
        | head -1 \
        | sed "s/.*=\s*[\"']\{0,1\}//; s/[\"']\s*$//")
    if [ -f "$TOKEN_FILE" ]; then
        cp "$TOKEN_FILE" "$DIST_DIR/"
        echo "    Copied token file: $TOKEN_FILE"
    else
        echo "    WARNING: api_token_file '$TOKEN_FILE' not found — copy it manually to $DIST_DIR/"
    fi
fi

SIZE=$(du -h "$DIST_DIR/$BINARY_NAME" | cut -f1)

echo ""
echo "==> Done!  $DIST_DIR/$BINARY_NAME  ($SIZE)"

# ── 6. Deploy binary to each instance on the Freebox ─────────────────────────
echo ""
echo "==> Deploying to ${REMOTE}..."
for INSTANCE in "${REMOTE_INSTANCES[@]}"; do
    echo "    scp -r $DIST_DIR/$BINARY_NAME ${REMOTE}:~/${INSTANCE}/"
    scp -r "$DIST_DIR/$BINARY_NAME" "${REMOTE}:~/${INSTANCE}/"
done
echo "==> Deploy complete."
echo ""
