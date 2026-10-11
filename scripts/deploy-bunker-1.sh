#!/bin/sh
set -e

BACKUP_DIR="deploy_backups"
ENGINE_BINARY="server-rs/target/release/server-rs"

mkdir -p "$BACKUP_DIR"

VERSION="1.1.58"
if [ -f "version.json" ]; then
    VERSION=$(grep -o '"version": "[^"]*"' version.json | cut -d'"' -f4 || echo "1.1.58")
fi

if [ -f "$ENGINE_BINARY" ]; then
    echo "📦 Archiving current binary version $VERSION to $BACKUP_DIR/server-rs_$VERSION"
    cp -f "$ENGINE_BINARY" "$BACKUP_DIR/server-rs_$VERSION"
fi

echo "🏗️ Compiling engine for Bunker 1..."
if [ -d "server-rs" ]; then
    (cd server-rs && cargo build --release)
else
    cargo build --release
fi

echo "✅ Deployment to Bunker 1 successful. New binary compiled."
