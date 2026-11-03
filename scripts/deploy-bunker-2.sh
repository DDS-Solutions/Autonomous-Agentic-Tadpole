#!/bin/sh
# @docs ARCHITECTURE:Core
#
# ### AI Assist Note
# **Core technical resource for the Tadpole OS Sovereign infrastructure.**
# Shell-level automation for the Tadpole OS deployment and maintenance.
#
# ### 🔍 Debugging & Observability
# - **Failure Path**: Environment error, permission denied, or command failure.
//! - **Telemetry Link**: Search `[deploy_bunker_2]` in system logs.

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

echo "🏗️ Compiling engine for Bunker 2..."
if [ -d "server-rs" ]; then
    (cd server-rs && cargo build --release)
else
    cargo build --release
fi

echo "✅ Deployment to Bunker 2 successful. New binary compiled."
# Metadata: [deploy_bunker_2]
