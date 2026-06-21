# Configuration Reference

## Overview

Introvert uses multiple configuration sources:
1. **Cargo.toml** — Rust dependencies and build settings
2. **pubspec.yaml** — Flutter dependencies and assets
3. **Build files** — Platform-specific build configuration
4. **Runtime config** — Environment variables and CLI arguments

## Cargo.toml

### Package Configuration
```toml
[package]
name = "introvert"
version = "0.1.0"
edition = "2021"
```

### Dependencies
Key dependencies and their purposes:

| Dependency | Version | Purpose |
|------------|---------|---------|
| libp2p | 0.56 | P2P networking |
| rusqlite | 0.31 | SQLCipher database |
| snow | 0.9 | Noise encryption |
| solana-sdk | 4.0.1 | Blockchain integration |
| tokio | 1.36 | Async runtime |
| serde | 1.0 | Serialization |

### Build Configuration
```toml
[lib]
name = "introvert"
path = "src/lib.rs"
crate-type = ["cdylib", "staticlib", "rlib"]

[[bin]]
name = "introvertd"
path = "src/main.rs"
```

## pubspec.yaml

### Project Configuration
```yaml
name: introvert_tests
description: Integration tests for the Introvert engine.
version: 1.0.0
publish_to: none
environment:
  sdk: '>=3.3.0 <4.0.0'
```

### Flutter Configuration
```yaml
flutter:
  config:
    enable-swift-package-manager: false
  uses-material-design: true
  assets:
    - assets/images/logo.png
    - assets/images/logo_black.png
    - assets/images/logo_white.png
    - assets/images/icon_transparent.png
    - assets/images/default_avatar.png
    - assets/images/app_icon.png
    - assets/images/introvert_wallpaper.png
    - assets/images/stickers/
    - assets/audio/introvert_ping.m4a
```

### Launcher Icons
```yaml
flutter_launcher_icons:
  android: "launcher_icon"
  ios: true
  macos:
    generate: true
  windows:
    generate: true
  image_path: "assets/images/app_icon_swarm_2.png"
  adaptive_icon_background: "#1E1E1E"
  adaptive_icon_foreground: "assets/images/app_icon_swarm_2.png"
```

### Native Splash
```yaml
flutter_native_splash:
  color: "#1E1E1E"
  image: "assets/images/app_icon_swarm_2.png"
  android_12:
    image: "assets/images/app_icon_swarm_2.png"
    color: "#1E1E1E"
```

## Android Configuration

### app/build.gradle.kts
```kotlin
android {
    namespace = "com.example.introvert_tests"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "com.example.introvert_tests"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }
}
```

### gradle.properties
```properties
org.gradle.jvmargs=-Xmx8G -XX:MaxMetaspaceSize=4G -XX:ReservedCodeCacheSize=512m
android.useAndroidX=true
android.builtInKotlin=false
android.newDsl=false
```

### AndroidManifest.xml Permissions
```xml
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
<uses-permission android:name="android.permission.WAKE_LOCK" />
<uses-permission android:name="android.permission.RECORD_AUDIO" />
<uses-permission android:name="android.permission.CAMERA" />
<uses-permission android:name="android.permission.MODIFY_AUDIO_SETTINGS" />
<uses-permission android:name="android.permission.MANAGE_OWN_CALLS"/>
<uses-permission android:name="android.permission.FOREGROUND_SERVICE"/>
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_PHONE_CALL"/>
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />
<uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION" />
<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
<uses-permission android:name="android.permission.VIBRATE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_SPECIAL_USE" />
```

## iOS Configuration

### Info.plist Keys
```xml
<key>NSLocalNetworkUsageDescription</key>
<string>Introvert uses local network to discover and connect to nearby peers for direct file transfer and messaging.</string>

<key>NSBonjourServices</key>
<array>
    <string>_ipfs._udp</string>
    <string>_ipfs._tcp</string>
</array>

<key>NSCameraUsageDescription</key>
<string>Introvert needs camera access for video calls and sharing photos.</string>

<key>NSMicrophoneUsageDescription</key>
<string>Introvert needs microphone access for voice calls and voice messages.</string>

<key>NSPhotoLibraryUsageDescription</key>
<string>Introvert needs photo library access to share images with contacts.</string>
```

### Podfile
```ruby
platform :ios, '13.0'

ENV['COCOAPODS_DISABLE_STATS'] = 'true'

target 'Runner' do
  use_frameworks!
  flutter_install_all_ios_pods File.dirname(File.realpath(__FILE__))
end
```

## macOS Configuration

### Podfile
```ruby
platform :osx, '10.15'

ENV['COCOAPODS_DISABLE_STATS'] = 'true'

target 'Runner' do
  use_frameworks!
  flutter_install_all_macos_pods File.dirname(File.realpath(__FILE__))
end

post_install do |installer|
  installer.pods_project.targets.each do |target|
    flutter_additional_macos_build_settings(target)
    target.build_configurations.each do |config|
      config.build_settings['MACOSX_DEPLOYMENT_TARGET'] = '10.15'
    end
  end
end
```

## RBN Daemon Configuration

### CLI Arguments
```bash
introvertd [OPTIONS]

Options:
  -s, --seed-file <PATH>        Path to 32-byte master seed file
  -d, --db-path <PATH>          Path to SQLCipher database [default: introvert.db]
  -p, --port <PORT>             TCP port to listen on [default: 443]
  -r, --relay                   Enable global relay server functionality
      --max-connections <N>      Maximum concurrent connections [default: 1000000]
      --liveness-check <SECS>   K-Bucket liveness check interval [default: 300]
      --data-dir <DIR>          Legacy data directory path
      --tunnel-port <PORT>      WebSocket tunnel port [default: 80]
```

### systemd Service
```ini
[Unit]
Description=Introvert Root Bootstrap Node (RBN) Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=/opt/introvert
ExecStart=/opt/introvert/bin/introvertd --data-dir /opt/introvert/data --relay --port 443
Environment="RUST_LOG=info"
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

## Analysis Options

### analysis_options.yaml
```yaml
include: package:flutter_lints/flutter.yaml

analyzer:
  exclude:
    - "stable_v*/**"
    - "plugins/**"

linter:
  rules:
    # Custom rules can be added here
```

## Makefile

### Build Targets
```makefile
mac:        # Build for macOS
android:    # Build for Android (arm64 + x86_64)
ios:        # Build for iOS (device + simulator)
all:        # Build for all platforms
clean:      # Remove build artifacts
```

## Network Configuration

### Dynamic Blockchain Bootstrapping
As of Phase 2, Introvert no longer uses hardcoded bootstrap node arrays. On startup, the Rust networking module (`src/network/service.rs`) queries the Solana `introvert-registry` program to fetch active RBN multiaddresses dynamically.

**Lookup Procedure:**
1. Connect to a high-uptime Solana RPC cluster
2. Query all program accounts owned by the `introvert-registry` address
3. Parse `Multiaddr` strings and validation metrics
4. Filter entries: must have active status AND >= 50,000 $INTR stake
5. Inject verified multiaddresses into Kademlia DHT swarm

### Legacy Bootstrap (Fallback)
```rust
// src/network/config.rs — Used only as fallback if Solana RPC is unreachable
pub fn get_bootstrap_nodes() -> Vec<(PeerId, Multiaddr)> {
    vec![
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(),
         "/ip4/47.89.252.80/tcp/443".to_string()),
    ]
}
```

### Extra Bootstrap Nodes
```bash
# Environment variable for additional bootstrap nodes
export INTROVERT_EXTRA_BOOTSTRAP="/ip4/x.x.x.x/tcp/443/p2p/PeerId"
```

### Token Gating Thresholds
| Tier | $INTR Minimum | Capability |
|------|---------------|------------|
| Edge Relay | 500 $INTR | Active P2P background relay (Event Code 22) |
| RBN Operator | 50,000 $INTR | Register as Root Bootstrap Node |

### Kademlia DHT Config
```rust
let mut kad_config = kad::Config::new(StreamProtocol::new("/introvert/kad/1.0.0"));
kad_config.set_record_ttl(Some(Duration::from_secs(24 * 60 * 60)));  // 24 hours
kad_config.set_publication_interval(Some(Duration::from_secs(60 * 60)));  // 1 hour
kad_config.set_replication_factor(NonZeroUsize::new(5).unwrap());
```

## Theme Configuration

### Built-in Themes
```dart
static const List<ThemeConfig> themes = [
  ThemeConfig(
    name: "Introvert Dark",
    bg: Color(0xFF0A0E17),
    surface: Color(0xFF1A1F2B),
    text: Colors.white,
    mutedText: Colors.white54,
    accent: Color(0xFF1AFFFF),
  ),
  // ... 4 more themes
];
```

## Intro-Claw AI Engine Configuration

### Overview
Intro-Claw is the local intelligence layer of Introvert — an on-device automation engine, AI assistant, network healer, and semantic search system. It runs deterministic, rule-based maintenance tasks on timers with zero network calls in Offline mode. The engine orchestrates 12 optimization modules via a 5-minute tick loop in NetworkService, provides a natural language assistant for querying app data, performs network reconnaissance and multi-strategy connection healing, and optionally integrates with external LLMs for advanced query understanding.

### Runtime Settings
The Intro-Claw automation engine has two operating modes controlled by a persistent toggle:

| Setting | Key | Values | Default |
|---------|-----|--------|---------|
| AI Mode | `intro_claw_ai_mode` | `0` = 100% Offline (Deterministic), `1` = Hybrid AI Assistant | `0` |
| API Key | `intro_claw_api_key` | Encrypted string (SQLCipher) | Empty |

### Storage Location
Settings are stored in the `economy_meta` table (key-value store):
```sql
INSERT INTO economy_meta (key, value) VALUES ('intro_claw_ai_mode', '0')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO economy_meta (key, value) VALUES ('intro_claw_api_key', '<encrypted_key>')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
```

### Architecture
- **Single module**: `src/intro_claw.rs` — all 12 sub-modules in one file
- **Tick loop**: 5-minute interval via `NetworkCommand::IntroClawTick` in NetworkService
- **Orchestrator**: `IntroClawService::tick()` calls all modules in sequence
- **Offline guard**: Every module checks `is_active` first — Offline mode = no work

### Automation Modules

#### 1. Battery-Saver Network Throttling
- **Thresholds**: Low=20%, Critical=10%
- **Scales**: mailbox_fetch (2min→10min→20min), heartbeat (30s→2min), contact_refresh (2min→10min), max_connections (1024→32→16)
- **Trigger**: Every tick (5 min), reads battery %, background state, peer count

#### 2. Database Pruning & Cache Cleaning
- **Prunes**: session_cache (>24h), crypto_sessions (>7d), mesh_chunks (>7d)
- **Optimizes**: `PRAGMA optimize` on SQLCipher hourly
- **Storage methods**: `prune_expired_sessions()`, `prune_expired_crypto_sessions()`, `run_pragma_optimize()`

#### 3. Media Lifecycle & Storage Management
- **Cleanup**: Orphaned mesh_chunks not in drive_files (every 30 min)
- **Quota**: Auto-prunes at 80% device disk, aggressive prune at 90%
- **Storage methods**: `cleanup_orphaned_mesh_chunks()`, `get_active_drive_hashes()`, `get_storage_usage()`

#### 4. Connection Optimization
- **Logic**: mDNS + battery checks for direct P2P upgrade candidates
- **Trigger**: Every 5 min, skips on critical battery
- **Action**: Logs upgrade candidates for future NetworkCommand integration

#### 5. Smart Message Batching
- **Logic**: Queues outgoing messages during poor connectivity (battery throttle active)
- **Auto-flush**: Drains queue when conditions improve or batch exceeds 50 messages
- **API**: `queue_batch()`, `flush_batch()`, `should_batch()`

#### 6. Predictive File Pre-fetching
- **Logic**: Scans top 5 contacts' recent messages for `[FILE]:` entries
- **Missing files**: Checks against drive_files, schedules StartPull for missing hashes
- **Limit**: 3 concurrent prefetches max
- **Trigger**: Every 5 min

#### 7. Smart Sync Prioritization
- **Logic**: Sorts contacts by unread count descending, syncs top 3 first
- **Trigger**: Every 2 min
- **API**: `prioritize()`, `next_peer()`

#### 8. Duplicate Message Suppression
- **Logic**: Vec<String> with 10k capacity, FIFO eviction
- **Check**: `check(msg_id)` before storing messages
- **Mark**: `mark_seen(msg_id)` after successful storage
- **Passive**: Runs on every message write, not on tick

#### 9. Connection Health Scoring
- **Logic**: Decay-based scoring (0.9 decay, 0.1 boost) per peer
- **Scores**: 0.0-1.0 range, feeds into ConnectionOptimizer
- **API**: `record_success()`, `record_failure()`, `get_score()`

#### 10. Storage Quota Management
- **Thresholds**: Warning at 80%, Critical at 90%
- **Action**: Auto-prune mesh_chunks at critical, orphan cleanup at warning
- **Check**: Every 30 min via MediaLifecycleManager

#### 11. Adaptive Chunk Sizing
- **Logic**: Tracks throughput per peer (last 10 observations)
- **Thresholds**: >10MB/s→512KB, >1MB/s→256KB, <1MB/s→64KB
- **API**: `get_optimal_chunk_size()`, `record_throughput()`
- **Passive**: Called during file transfers

#### 12. Tick Loop Integration
- **Timer**: 5-min interval in NetworkService `run()` select loop
- **Context**: Battery %, background state, connected peers, mDNS discoveries, active transfers
- **Handler**: `NetworkCommand::IntroClawTick` processes all 12 modules

### FFI Functions
| Function | Signature | Purpose |
|----------|-----------|---------|
| `intro_claw_get_ai_mode` | `() -> i32` | Get current mode (0/1) |
| `intro_claw_set_ai_mode` | `(mode: i32, api_key: *const c_char) -> FfiResult` | Set mode + optional API key |
| `intro_claw_get_api_key` | `() -> *mut c_char` | Get encrypted API key |
| `intro_claw_trigger_tick` | `() -> FfiResult` | Manual maintenance cycle |
| `intro_claw_set_active` | `(active: bool) -> FfiResult` | Enable/disable engine |
| `intro_claw_get_status` | `() -> FfiResult` | JSON status report |
| `intro_claw_get_endpoint` | `() -> *mut c_char` | Get LLM endpoint URL |
| `intro_claw_set_endpoint` | `(endpoint: *const c_char) -> FfiResult` | Set LLM endpoint URL |
| `intro_claw_process_query` | `(query: *const c_char) -> FfiResult` | Process natural language query |
| `intro_claw_run_network_recon` | `() -> FfiResult` | Network recon report (markdown) |
| `intro_claw_heal_peer` | `(peer_id: *const c_char) -> FfiResult` | Multi-strategy connection healing |

### Dart FFI Bridge
```dart
// Type definitions
typedef IntroClawGetAiModeC = Int32 Function();
typedef IntroClawSetAiModeC = FfiResult Function(Int32 mode, Pointer<Utf8> apiKey);
typedef IntroClawGetApiKeyC = Pointer<Utf8> Function();
typedef IntroClawTriggerTickC = FfiResult Function();
typedef IntroClawSetActiveC = FfiResult Function(Bool active);
typedef IntroClawGetStatusC = FfiResult Function();

// Client methods
int getIntroClawAiMode() => _getAiMode();
void setIntroClawAiMode(int mode, {String apiKey = ''}) { ... }
String getIntroClawApiKey() { ... }
void triggerIntroClawTick() => _handleFfiResult(_clawTriggerTick(), context: "IntroClaw Tick");
void setIntroClawActive(bool active) => _handleFfiResult(_clawSetActive(active), context: "IntroClaw Active");
String getIntroClawStatus() { ... }
```

### Settings UI
- **Section**: "INTRO-CLAW AUTOMATION ENGINE" in Settings tab
- **Mode Toggle**: Radio-style selector (Offline/Hybrid)
- **API Key**: Text field with visibility toggle (Hybrid mode only)
- **Status Dashboard**: Engine status, last tick, storage usage
- **Maintenance Button**: Triggers immediate tick cycle
- **CLAW Tab**: Chat interface for natural language queries, RECON button for network diagnostics, HEAL button for connection recovery, Info panel

### Semantic Intent Engine (BERT Embeddings)
- **Model**: `sentence-transformers/all-MiniLM-L6-v2` (downloaded on first run, ~23MB)
- **Framework**: `candle-core` + `candle-transformers` (pure Rust, CPU inference)
- **Tokenization**: `tokenizers` crate (HuggingFace format)
- **Inference pipeline**: Tokenize → BERT forward pass → mean pooling → L2 normalization → 384-dim vectors
- **12 action intents**: battery_throttle, db_prune, media_cleanup, connection_optimize, message_batch, prefetch, sync_priority, dedup, health_score, storage_quota, adaptive_chunk, tick
- **Matching**: Keyword scoring (instant) → cosine similarity (when model loaded) → threshold 0.75
- **Thread priority**: `libc::setpriority(PRIO_PROCESS, 0, 10)` — low priority background thread

### Network Recon & Healing
- **Recon**: Collects mesh state, peer routing, connection types, anchor status, storage usage → formats as monospaced markdown report
- **Healing strategies** (executed sequentially):
  1. Direct libp2p dial (2s timeout)
  2. Relay circuit v2 via RBN (3s timeout)
  3. Anchor node routing
  4. WebSocket tunnel activation
  5. Persistent mailbox fallback
- **UI**: Terminal-style milestone animation, green-on-black code block rendering

### Offline Mode Behavior
When `intro_claw_ai_mode == 0` (100% Offline):
- No outbound HTTP, WebSocket, or external network calls from intro-claw
- Falls back entirely to local database pruning, network throttling, and deterministic keyword scripts
- Compilation pipeline must enforce structural block preventing network modules under intro-claw namespace

### Security Constraints (Non-Negotiable)
- Zero access to Master Cryptographic Seed or raw HKDF-SHA256 derived keys
- No raw cryptographic secrets — limited to invoking pre-existing Introvert functions
- Hard compile-time failure on secret exposure (crypto_sessions, session_cache, group_secrets)
- Least privilege: application performance maintenance only
- No unbound sockets or unhashed user identifiers
- In Offline Mode: zero outbound data traffic (compile-time enforced)
