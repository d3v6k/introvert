#!/usr/bin/env bash
# ==============================================================================
# Introvert Comprehensive Backup System
# ==============================================================================
# Naming: MM_YY_HHMM format (e.g., 07_06_1430)
# Process: 1) Update docs → 2) Sync all code → 3) Verify completeness
# Recovery: Full restore from single backup folder — no external dependencies
#
# Usage: ./scripts/backup.sh
#        make bk

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m'

PROJECT_DIR="/Users/dev/Development/introvert"
BACKUP_ROOT="/Volumes/512-SSD-External/introvert back up"
BACKUP_NAME="$(date +%d_%m_%y_%H%M)"
BACKUP_DIR="$BACKUP_ROOT/$BACKUP_NAME"

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║          INTROVERT COMPREHENSIVE BACKUP SYSTEM              ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Backup:${NC} $BACKUP_NAME"
echo -e "${BLUE}Location:${NC} $BACKUP_DIR"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# STEP 1: Update project documentation before backup
# ═══════════════════════════════════════════════════════════════════════════════
echo -e "${BLUE}📝 Step 1/3: Updating project documentation...${NC}"

TIMESTAMP=$(date '+%Y-%m-%d %H:%M')
GIT_HASH=$(cd "$PROJECT_DIR" && git rev-parse --short HEAD 2>/dev/null || echo "no-git")
GIT_BRANCH=$(cd "$PROJECT_DIR" && git branch --show-current 2>/dev/null || echo "no-git")

# Update VERSION_CHANGELOG.md with current session info
if [ -f "$PROJECT_DIR/VERSION_CHANGELOG.md" ]; then
  echo "" >> "$PROJECT_DIR/VERSION_CHANGELOG.md"
  echo "## Backup $BACKUP_NAME ($TIMESTAMP)" >> "$PROJECT_DIR/VERSION_CHANGELOG.md"
  echo "- Git: $GIT_BRANCH @ $GIT_HASH" >> "$PROJECT_DIR/VERSION_CHANGELOG.md"
  echo "- Machine: $(hostname)" >> "$PROJECT_DIR/VERSION_CHANGELOG.md"
  echo -e "  ${GREEN}✓${NC} VERSION_CHANGELOG.md updated"
fi

# Update DEBUG_DOCUMENT.md with current status
if [ -f "$PROJECT_DIR/DEBUG_DOCUMENT.md" ]; then
  echo "" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo "---" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo "## Backup Status ($TIMESTAMP)" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo "- Git: $GIT_BRANCH @ $GIT_HASH" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo "- RBN: introvertd on 47.89.252.80:443" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo "- Economy: introvert-solana on localhost:9001" >> "$PROJECT_DIR/DEBUG_DOCUMENT.md"
  echo -e "  ${GREEN}✓${NC} DEBUG_DOCUMENT.md updated"
fi

echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# STEP 2: Sync all project files (excluding build artifacts)
# ═══════════════════════════════════════════════════════════════════════════════
echo -e "${BLUE}📦 Step 2/3: Syncing all project files...${NC}"

mkdir -p "$BACKUP_DIR"

BACKED_UP=0

# Helper: rsync with status
sync_dir() {
  local src="$1" dst="$2" label="$3"
  shift 3
  if [ -d "$src" ]; then
    rsync -av "$@" "$src/" "$dst/" > /dev/null 2>&1
    echo -e "  ${GREEN}✓${NC} $label"
    BACKED_UP=$((BACKED_UP + 1))
  fi
}

# Helper: copy file with status
copy_file() {
  local src="$1" dst="$2" label="$3"
  if [ -f "$src" ]; then
    cp "$src" "$dst/"
    echo -e "  ${GREEN}✓${NC} $label"
    BACKED_UP=$((BACKED_UP + 1))
  fi
}

# ── Core source code ─────────────────────────────────────────────────────────
# Client Rust engine
sync_dir "$PROJECT_DIR/src" "$BACKUP_DIR/src" "Client Rust (src/)" \
  --exclude='target/' --exclude='.DS_Store'

# Flutter/Dart UI
sync_dir "$PROJECT_DIR/lib" "$BACKUP_DIR/lib" "Flutter UI (lib/)" \
  --exclude='.dart_tool/' --exclude='build/' --exclude='.DS_Store'

# RBN daemon
sync_dir "$PROJECT_DIR/for_linux" "$BACKUP_DIR/for_linux" "RBN daemon (for_linux/)" \
  --exclude='target/' --exclude='.DS_Store'

# Economy daemon
sync_dir "$PROJECT_DIR/introvert-daemon" "$BACKUP_DIR/introvert-daemon" "Economy daemon (introvert-daemon/)" \
  --exclude='target/' --exclude='.DS_Store'

# Swarm Marshal (token deployment)
sync_dir "$PROJECT_DIR/introvert-token" "$BACKUP_DIR/introvert-token" "Swarm Marshal (introvert-token/)" \
  --exclude='target/' --exclude='.DS_Store'

# P2P crate
sync_dir "$PROJECT_DIR/introvert-p2p" "$BACKUP_DIR/introvert-p2p" "P2P crate (introvert-p2p/)" \
  --exclude='target/' --exclude='.DS_Store'

# Solana crate
sync_dir "$PROJECT_DIR/introvert-solana" "$BACKUP_DIR/introvert-solana" "Solana crate (introvert-solana/)" \
  --exclude='target/' --exclude='.DS_Store'

# ── Platform shells ──────────────────────────────────────────────────────────
for dir in android ios macos linux web windows; do
  sync_dir "$PROJECT_DIR/$dir" "$BACKUP_DIR/$dir" "Platform: $dir" \
    --exclude='build/' --exclude='.gradle/' --exclude='Pods/' --exclude='.dart_tool/' --exclude='.DS_Store'
done

# ── Assets, scripts, config ─────────────────────────────────────────────────
sync_dir "$PROJECT_DIR/assets" "$BACKUP_DIR/assets" "Assets"
sync_dir "$PROJECT_DIR/scripts" "$BACKUP_DIR/scripts" "Scripts"
sync_dir "$PROJECT_DIR/firebase" "$BACKUP_DIR/firebase" "Firebase config"
sync_dir "$PROJECT_DIR/solana_program" "$BACKUP_DIR/solana_program" "Solana program" \
  --exclude='test-ledger/' --exclude='target/' --exclude='.DS_Store'
sync_dir "$PROJECT_DIR/Docs" "$BACKUP_DIR/Docs" "Documentation"
sync_dir "$PROJECT_DIR/plugins" "$BACKUP_DIR/plugins" "Plugins" \
  --exclude='build/' --exclude='.cxx/' --exclude='.DS_Store'

# ── Root-level Dart files (flat structure compatibility) ─────────────────────
for f in main.dart blueprint_ui.dart connectivity_listener.dart; do
  copy_file "$PROJECT_DIR/$f" "$BACKUP_DIR" "$f"
done

# ── Root-level Rust files (flat structure compatibility) ─────────────────────
for f in lib.rs storage.rs identity.rs intro_claw.rs embedding.rs main.rs test_add_addr.rs; do
  copy_file "$PROJECT_DIR/$f" "$BACKUP_DIR" "$f"
done

# ── Root-level directories (flat structure compatibility) ────────────────────
for dir in views theme Components Economy Operations Protocol Marketing Audits Releases Solana architecture programs media network anchor bin; do
  sync_dir "$PROJECT_DIR/$dir" "$BACKUP_DIR/$dir" "$dir/"
done

# ── Build/deploy scripts ────────────────────────────────────────────────────
for script in build_android.sh build_linux.sh cmake_wrapper.sh build_standalone_apk.sh setup_rbn.sh deploy_rbn.sh deploy_local_rbn.sh deploy_anchor.sh deploy_first_light.sh deploy_introvert_token.sh build_and_deploy.sh finalize_rbn.sh; do
  copy_file "$PROJECT_DIR/$script" "$BACKUP_DIR" "$script"
done

# ── Config files ─────────────────────────────────────────────────────────────
for f in Anchor.toml CMakeLists.txt dashboard.html google-services.json GEMINI_REMOTE_PROMPT.txt; do
  copy_file "$PROJECT_DIR/$f" "$BACKUP_DIR" "$f"
done

# ── Compiled binaries ────────────────────────────────────────────────────────
copy_file "$PROJECT_DIR/libintrovert.dylib" "$BACKUP_DIR" "libintrovert.dylib"
copy_file "$PROJECT_DIR/introvertd" "$BACKUP_DIR" "introvertd"
copy_file "$PROJECT_DIR/introvertd.service" "$BACKUP_DIR" "introvertd.service"

# ── Android google-services.json ─────────────────────────────────────────────
copy_file "$PROJECT_DIR/android/app/google-services.json" "$BACKUP_DIR" "android/app/google-services.json"

# ── Firebase service account keys ────────────────────────────────────────────
for f in "$PROJECT_DIR"/introvert-p2p-firebase-adminsdk-*.json; do
  if [ -f "$f" ]; then
    cp "$f" "$BACKUP_DIR/"
    echo -e "  ${GREEN}✓${NC} $(basename "$f")"
    BACKED_UP=$((BACKED_UP + 1))
  fi
done

# ── Project config ───────────────────────────────────────────────────────────
copy_file "$PROJECT_DIR/Cargo.toml" "$BACKUP_DIR" "Cargo.toml"
copy_file "$PROJECT_DIR/Cargo.lock" "$BACKUP_DIR" "Cargo.lock"
copy_file "$PROJECT_DIR/pubspec.yaml" "$BACKUP_DIR" "pubspec.yaml"
copy_file "$PROJECT_DIR/pubspec.lock" "$BACKUP_DIR" "pubspec.lock"
copy_file "$PROJECT_DIR/analysis_options.yaml" "$BACKUP_DIR" "analysis_options.yaml"
copy_file "$PROJECT_DIR/Makefile" "$BACKUP_DIR" "Makefile"
copy_file "$PROJECT_DIR/.gitignore" "$BACKUP_DIR" ".gitignore"

# ── Documentation ────────────────────────────────────────────────────────────
for doc in README.md VERSION_CHANGELOG.md CHANGELOG.md \
  ARCHITECTURE_BLUEPRINT.md CONFIGURATION_REFERENCE.md \
  CROSS_NETWORK_FAILURE_REMEDIATION_PLAN.md FIX_PLAN_RELAY_MESSAGING.md \
  NETWORKING_STABILIZATION_PLAN.md INTROVERT_MASTER_PLAN.md \
  INTEGRATION_SUMMARY.md GEMINI.md \
  DEBUG_DOCUMENT.md DEBUG_SESSION_STATUS.md SESSION_HANDOFF.md \
  RELEASE_NOTES.md RELEASE_NOTES_v22.md RELEASE_NOTES_v25.md \
  RELEASE_NOTES_v29.md RELEASE_NOTES_v39.md \
  BACKUP_LOCATION.md BACKUP_SUMMARY.md INTROVERT_MANIFESTO.md; do
  copy_file "$PROJECT_DIR/$doc" "$BACKUP_DIR" "$doc"
done

echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# STEP 3: Generate recovery manifest and verify completeness
# ═══════════════════════════════════════════════════════════════════════════════
echo -e "${BLUE}📋 Step 3/3: Generating recovery manifest...${NC}"

TOTAL_SIZE=$(du -sh "$BACKUP_DIR" | cut -f1)
ITEM_COUNT=$(find "$BACKUP_DIR" -type f | wc -l | tr -d ' ')
SRC_COUNT=$(find "$BACKUP_DIR/src" -name '*.rs' -type f 2>/dev/null | wc -l | tr -d ' ')
LIB_COUNT=$(find "$BACKUP_DIR/lib" -name '*.dart' -type f 2>/dev/null | wc -l | tr -d ' ')
FOR_LINUX_COUNT=$(find "$BACKUP_DIR/for_linux" -name '*.rs' -type f 2>/dev/null | wc -l | tr -d ' ')

cat > "$BACKUP_DIR/BACKUP_SUMMARY.md" << EOF
# Backup Recovery Manifest — $BACKUP_NAME

**Created:** $TIMESTAMP
**Git:** $GIT_BRANCH @ $GIT_HASH
**Machine:** $(hostname)
**User:** $(whoami)

## File Counts
- Total files: $ITEM_COUNT
- Client Rust (src/): $SRC_COUNT .rs files
- Flutter UI (lib/): $LIB_COUNT .dart files
- RBN daemon (for_linux/): $FOR_LINUX_COUNT .rs files
- Size: $TOTAL_SIZE

## Contents

### Source Code (all platforms)
- \`src/\` — Client Rust core (libp2p, networking, economy, storage, FFI)
- \`lib/\` — Flutter UI (Dart, views, widgets, services)
- \`for_linux/\` — RBN daemon (relay, reward engine, FCM, WebSocket tunnel)
- \`introvert-daemon/\` — Economy daemon (Solana, treasury, IPC)
- \`introvert-p2p/\` — P2P crate
- \`introvert-solana/\` — Solana crate
- \`android/\`, \`ios/\`, \`macos/\`, \`linux/\`, \`web/\`, \`windows/\` — Platform shells

### Configuration
- \`Cargo.toml\` + \`Cargo.lock\` — Rust dependencies (pinned)
- \`pubspec.yaml\` + \`pubspec.lock\` — Flutter dependencies (pinned)
- \`Makefile\` — Build commands (mac, android, ios, all, bk)
- \`firebase/\` — Firebase service account + config
- \`solana_program/\` — Anchor program
- \`plugins/\` — Local Flutter plugins (pdf_render_maintained)
- \`google-services.json\` — Android Firebase config

### Compiled Binaries
- \`libintrovert.dylib\` — macOS native library
- \`introvertd\` — RBN daemon binary (Linux x86_64)
- \`introvertd.service\` — systemd service file

### Deploy Scripts
- \`deploy_rbn.sh\` — Build on thinkpad → deploy to Alibaba RBN
- \`deploy_local_rbn.sh\` — Local RBN deployment
- \`build_android.sh\` — Android cross-compilation

### Documentation
- \`Docs/\` — Full documentation folder
- \`README.md\`, \`VERSION_CHANGELOG.md\`, \`DEBUG_DOCUMENT.md\`
- \`BACKUP_SUMMARY.md\` — This file

## Recovery Procedure

### From this backup to working app:
1. Copy this folder to \`/Users/dev/Development/introvert/\`
2. Install Rust: \`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\`
3. Install Flutter: https://docs.flutter.dev/get-started/install
4. Install Android SDK + NDK 28.2.13676358
5. \`flutter pub get\` — install Dart dependencies
6. \`make mac\` — build macOS native library
7. \`make android\` — build Android native libraries (requires NDK)
8. \`make ios\` — build iOS static libraries
9. \`flutter run -d macos\` — run on macOS
10. \`flutter run -d <device>\` — run on Android

### Deploy RBN:
1. \`./deploy_rbn.sh\` — syncs to thinkpad, compiles, deploys to 47.89.252.80

### What this backup preserves:
- All source code (client, RBN, economy daemon)
- All platform configurations (Android, iOS, macOS, Linux, Web, Windows)
- All dependencies (Cargo.lock, pubspec.lock — pinned versions)
- All Firebase/Solana config
- All compiled binaries (dylib, .so, .a, introvertd)
- All deploy scripts
- All documentation
- Git history (via .git directory — not included in backup, use git remote)

### What is NOT in this backup:
- \`.git/\` directory (use git clone from remote)
- \`build/\`, \`target/\` directories (regenerated by build)
- \`node_modules/\` (regenerated by flutter pub get)
- SQLCipher database (user data — separate backup needed)
- User's master seed (security-sensitive — separate backup needed)
EOF

echo -e "  ${GREEN}✓${NC} BACKUP_SUMMARY.md generated"

# ── Verify critical files exist ──────────────────────────────────────────────
echo ""
echo -e "${BLUE}🔍 Verifying backup completeness...${NC}"

ERRORS=0

# Check critical source directories
for dir in src lib for_linux; do
  if [ ! -d "$BACKUP_DIR/$dir" ]; then
    echo -e "  ${RED}✗${NC} MISSING: $dir/"
    ERRORS=$((ERRORS + 1))
  fi
done

# Check critical files
for f in Cargo.toml pubspec.yaml Makefile; do
  if [ ! -f "$BACKUP_DIR/$f" ]; then
    echo -e "  ${RED}✗${NC} MISSING: $f"
    ERRORS=$((ERRORS + 1))
  fi
done

# Check critical Rust files
for f in src/lib.rs src/economy/mod.rs src/network/mod.rs; do
  if [ ! -f "$BACKUP_DIR/$f" ]; then
    echo -e "  ${RED}✗${NC} MISSING: $f"
    ERRORS=$((ERRORS + 1))
  fi
done

# Check critical Dart files
for f in lib/main.dart lib/src/ui/main_shell.dart lib/src/native/introvert_client.dart; do
  if [ ! -f "$BACKUP_DIR/$f" ]; then
    echo -e "  ${RED}✗${NC} MISSING: $f"
    ERRORS=$((ERRORS + 1))
  fi
done

# Check RBN daemon files
for f in for_linux/src/main.rs for_linux/src/network/mod.rs for_linux/src/economy/daily_rewards.rs; do
  if [ ! -f "$BACKUP_DIR/$f" ]; then
    echo -e "  ${RED}✗${NC} MISSING: $f"
    ERRORS=$((ERRORS + 1))
  fi
done

if [ $ERRORS -eq 0 ]; then
  echo -e "  ${GREEN}✓${NC} All critical files present"
else
  echo -e "  ${RED}✗${NC} $ERRORS critical files missing!"
fi

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    BACKUP COMPLETE                          ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Name:${NC} $BACKUP_NAME"
echo -e "${BLUE}Location:${NC} $BACKUP_DIR"
echo -e "${BLUE}Size:${NC} $TOTAL_SIZE"
echo -e "${BLUE}Files:${NC} $ITEM_COUNT"
echo -e "${BLUE}Errors:${NC} $ERRORS"
