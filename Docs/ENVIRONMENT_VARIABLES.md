# Environment Variables Reference

## Core Engine

### `INTROVERT_SEED`
**Purpose:** Load master seed from environment (daemon mode)
**Format:** 64-character hex string (32 bytes)
**Example:** `INTROVERT_SEED=a1b2c3d4e5f6...`
**Usage:** `src/main.rs` — Bypasses interactive seed prompt

### `INTROVERT_SKIP_BOOTSTRAP`
**Purpose:** Ignore all hardcoded bootstrap nodes
**Format:** Any value (presence enables)
**Example:** `INTROVERT_SKIP_BOOTSTRAP=1`
**Usage:** `src/network/config.rs` — Air-gapped/isolated testing

### `INTROVERT_EXTRA_BOOTSTRAP`
**Purpose:** Add custom bootstrap nodes without recompiling
**Format:** `PID1:ADDR1,PID2:ADDR2`
**Example:** `INTROVERT_EXTRA_BOOTSTRAP="12D3KooW...:/ip4/10.0.0.1/tcp/443"`
**Usage:** `src/network/config.rs` — Private mesh testing

### `INTROVERT_TRUST_ALL_WITNESSES`
**Purpose:** Allow handle registration via unauthorized anchors
**Format:** Any value (presence enables)
**Example:** `INTROVERT_TRUST_ALL_WITNESSES=1`
**Usage:** `for_linux/src/network/mod.rs` — Reduces quorum to 1

### `INTROVERT_TEST`
**Purpose:** Enable test mode configurations
**Format:** Any value (presence enables)
**Example:** `INTROVERT_TEST=1`
**Usage:** `for_linux/src/media/mod.rs` — Test ICE servers

## Android Build

### `ANDROID_HOME`
**Purpose:** Android SDK location
**Format:** Directory path
**Example:** `ANDROID_HOME=/Users/dev/Library/Android/sdk`
**Usage:** `scripts/build_android.sh`

### `ANDROID_NDK_HOME`
**Purpose:** Android NDK location
**Format:** Directory path
**Example:** `ANDROID_NDK_HOME=/Users/dev/Library/Android/sdk/ndk/28.2.13676358`
**Usage:** `scripts/build_android.sh`

### `ANDROID_ABI`
**Purpose:** Target Android ABI for build
**Format:** `arm64-v8a` or `x86_64`
**Example:** `ANDROID_ABI=arm64-v8a`
**Usage:** `scripts/build_android.sh`

## iOS Build

### `IPHONEOS_DEPLOYMENT_TARGET`
**Purpose:** Minimum iOS version
**Format:** Version string
**Example:** `IPHONEOS_DEPLOYMENT_TARGET=13.0`
**Usage:** `Makefile` — iOS compilation

## Flutter

### `FLUTTER_ROOT`
**Purpose:** Flutter SDK location
**Format:** Directory path
**Example:** `FLUTTER_ROOT=/opt/homebrew/share/flutter`
**Usage:** `ios/Podfile`, `macos/Podfile`

## RBN Daemon

### `RUST_LOG`
**Purpose:** Logging level for introvertd
**Format:** `info`, `debug`, `warn`, `error`
**Example:** `RUST_LOG=info`
**Usage:** systemd service configuration

### `FIREBASE_SERVICE_ACCOUNT_PATH`
**Purpose:** Path to Firebase service account credentials for FCM push notifications
**Format:** Absolute file path
**Example:** `FIREBASE_SERVICE_ACCOUNT_PATH=/opt/introvert/config/firebase-service-account.json`
**Usage:** `for_linux/src/fcm.rs` — Firebase Admin SDK initialization for FCM v1 API
**Default:** `/opt/introvert/config/firebase-service-account.json`

### `APNS_KEY_PATH`
**Purpose:** Path to Apple Push Notification service P8 key file
**Format:** Absolute file path
**Example:** `APNS_KEY_PATH=/opt/introvert/config/apns-key.p8`
**Usage:** `for_linux/src/fcm.rs` — APNs JWT authentication for iOS push notifications

### `APNS_KEY_ID`
**Purpose:** Apple Push Notification Key ID
**Format:** 10-character alphanumeric string
**Example:** `APNS_KEY_ID=ABC1234567`
**Usage:** `for_linux/src/fcm.rs` — APNs JWT header `kid` claim

### `APNS_TEAM_ID`
**Purpose:** Apple Developer Team ID
**Format:** 10-character alphanumeric string
**Example:** `APNS_TEAM_ID=XYZ9876543`
**Usage:** `for_linux/src/fcm.rs` — APNs JWT `iss` claim

### `APNS_BUNDLE_ID`
**Purpose:** iOS app bundle identifier
**Format:** Reverse-domain string
**Example:** `APNS_BUNDLE_ID=chat.introvert.app`
**Usage:** `for_linux/src/fcm.rs` — APNs `apns-topic` header
**Default:** `chat.introvert.app`

### `APNS_USE_PRODUCTION`
**Purpose:** Use APNs production or sandbox endpoint
**Format:** `true` or `false`
**Example:** `APNS_USE_PRODUCTION=true`
**Usage:** `for_linux/src/fcm.rs` — Selects `api.push.apple.com` vs `api.development.push.apple.com`
**Default:** `true`

## Development

### `INTROVERT_DEBUG`
**Purpose:** Enable debug output
**Format:** Any value (presence enables)
**Example:** `INTROVERT_DEBUG=1`
**Usage:** Various debug prints

## Usage Examples

### Local Development (macOS)
```bash
export INTROVERT_SEED=$(openssl rand -hex 32)
flutter run
```

### RBN Deployment
```bash
# In systemd service (/etc/systemd/system/introvertd.service)
Environment="RUST_LOG=info"
Environment="FIREBASE_SERVICE_ACCOUNT_PATH=/opt/introvert/config/firebase-service-account.json"
Environment="APNS_KEY_PATH=/opt/introvert/config/apns-key.p8"
Environment="APNS_KEY_ID=YOUR_KEY_ID"
Environment="APNS_TEAM_ID=YOUR_TEAM_ID"
Environment="APNS_BUNDLE_ID=chat.introvert.app"
ExecStart=/opt/introvert/bin/introvertd --data-dir /opt/introvert/data --relay --port 443
```

### Private Mesh Testing
```bash
export INTROVERT_EXTRA_BOOTSTRAP="12D3KooW...:/ip4/192.168.1.100/tcp/443"
export INTROVERT_TRUST_ALL_WITNESSES=1
./introvertd --relay --port 443
```

### Air-Gapped Mode
```bash
export INTROVERT_SKIP_BOOTSTRAP=1
./introvertd --port 443
```
