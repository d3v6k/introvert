#!/bin/bash
set -e

echo "🚀 Preparing release v0.30.0..."

# Check assets
if [ ! -f "build/app/outputs/flutter-apk/app-release.apk" ]; then
    echo "❌ Error: build/app/outputs/flutter-apk/app-release.apk not found. Please run 'flutter build apk --release' first."
    exit 1
fi

if [ ! -f "Introvert-macOS.dmg" ]; then
    echo "❌ Error: Introvert-macOS.dmg not found. Please run 'make macos-dmg' first."
    exit 1
fi

echo "✅ Release assets validated."

# Extract latest release notes from Docs/CHANGELOG.md
# We take the content under [0.30.0] up to the next version [0.29.0]
echo "📝 Extracting release notes..."
cat << 'EOF' > release_notes.tmp
# Introvert v0.30.0 - Sovereign Economy & Snappy Mesh

### Milestone
Fully aligned, signed, and validated client-to-RBN telemetry pipeline, database persistence for client telemetry, midnight UTC epoch close cron scheduler with automatic Solana treasury claims (HMAC-SHA256 IPC), and 15-second snappy connection recovery cycler integration.

### Key Changes
- **Cryptographic Telemetry Signing**: Added `package_signed_telemetry()` on client to sign telemetry metrics using Ed25519 with client's derived Solana keypair.
- **RBN Telemetry Validation**: Added signature validation on the RBN server node (`process_telemetry`) to authenticate declarations before scoring.
- **SQLite Telemetry Persistence**: Expanded RBN database schema with `client_telemetry` table and implemented `save_client_telemetry()` and `fetch_client_telemetry_for_epoch()` to securely store signed envelopes, surviving RBN daemon restarts.
- **13-Metrics Schema Alignment**: Expanded client shared metrics from `[u64; 9]` to `[u64; 13]` to include `WebFocusedActiveTime`, `SandboxWebPacketData`, `WebViewMediaCallHook`, and `UniquePeerHandshakes` across client economy tracker, network types, and stress tester.
- **Midnight UTC Scheduler**: Implemented tokio background loop on RBN to periodically check for midnight UTC, run `close_current_epoch()`, and generate claim payouts.
- **Solana Treasury IPC Claims**: Implemented `send_claim_to_treasury()` on RBN to sign daily claim request payloads with HMAC-SHA256 using `/etc/introvert/ipc.secret` and write them to the local `introvert-solana` daemon on port 9001.
- **Snappy Peer Reconnection**: Integrated connection state cycler (`ConnectionStateCycler`) evaluation into the 15-second status loop. Disconnected clients now rotate connection strategies (Direct re-dial, WebTunnel fallback, VPN profiles) immediately, resolving the 5-minute stuck-in-connecting bug.
- **Unit Test Outlier Preimages**: Fixed daily rewards and dual-pool separation unit tests by generating correct cryptographic proof hashes from preimage formats instead of using dummy strings.
- **Contact Sheet Reactions**: Supported applying emoji reactions to an entire contact sheet of images (represented by ImageGroupProgress) or any file/image stack. Reacting to the stack automatically distributes the reaction to all underlying images. Fully functional in both 1:1 and group chats.
EOF

echo "📦 Creating GitHub Release v0.30.0..."
gh release create v0.30.0 \
    build/app/outputs/flutter-apk/app-release.apk \
    Introvert-macOS.dmg \
    --title "v0.30.0 - Sovereign Economy & Snappy Mesh" \
    --notes-file release_notes.tmp

rm release_notes.tmp
echo "🎉 Release v0.30.0 successfully created and binaries uploaded!"
