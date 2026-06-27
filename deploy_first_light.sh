#!/usr/bin/env bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - First Light Validation
# ==============================================================================

set -euo pipefail

echo "🚀 Building First Light Native Core..."

# 1. Build Shared Library
# Updated to match 'leak-and-reclaim' memory architecture
cargo build --release

# 2. Strip debug symbols
strip target/release/libintrovert.so

# 3. Deploy to Linux integration path
mkdir -p linux/flutter/ephemeral
cp target/release/libintrovert.so linux/flutter/ephemeral/libintrovert.so

echo "✅ Validation Core Ready. Integration tests may proceed."
