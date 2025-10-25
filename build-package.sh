#!/bin/bash
# Safe package builder that doesn't interfere with the source tree

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="/tmp/hashsum-pkg-build-$$"

echo "Creating temporary build directory: $BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Copy project files to temp location
echo "Copying project files..."
cp -a "$SCRIPT_DIR/Cargo.toml" "$BUILD_DIR/" 2>/dev/null || true
cp -a "$SCRIPT_DIR/Cargo.lock" "$BUILD_DIR/" 2>/dev/null || true
cp -a "$SCRIPT_DIR/rust-toolchain.toml" "$BUILD_DIR/" 2>/dev/null || true
cp -a "$SCRIPT_DIR/PKGBUILD" "$BUILD_DIR/" 2>/dev/null || true
cp -a "$SCRIPT_DIR/src" "$BUILD_DIR/" 2>/dev/null || true

cd "$BUILD_DIR"

# Run makepkg with passed arguments (defaults to -si)
# Use BUILDDIR to avoid conflict with project's src/ directory
ARGS="${@:--si}"
echo "Running: FEATURES=\"${FEATURES:-}\" BUILDDIR=\"$BUILD_DIR/makepkg-work\" makepkg --cleanbuild $ARGS"
FEATURES="${FEATURES:-}" BUILDDIR="$BUILD_DIR/makepkg-work" makepkg --cleanbuild $ARGS

# Cleanup
echo "Cleaning up temporary directory..."
cd /
rm -rf "$BUILD_DIR"

echo "Done!"
