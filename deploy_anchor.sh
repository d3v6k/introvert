#!/usr/bin/env bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Anchor Daemon Deploy
# ==============================================================================

set -euo pipefail

echo "🛠️  Building Hardened Introvert Anchor Daemon (introvertd)..."

# 1. Build with production scale parameters
# Support for 1M connections and 5-minute liveness checks
cargo build --release --bin introvertd

# 2. Optimization
echo "✂️  Optimizing binary..."
strip target/release/introvertd

echo "✅ Compilation Successful."
echo "📍 Binary: ./target/release/introvertd"
echo ""
echo "🚀 Start your Anchor Node with Availability Yield enabled:"
echo "   ./target/release/introvertd --max-connections 1000000 --liveness-check 300"
