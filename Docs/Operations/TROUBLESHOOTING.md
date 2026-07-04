# Troubleshooting Guide

## Build Issues

### Rust Compilation Fails

**Symptom:** `cargo build` errors with OpenSSL or linker issues

**Solution:**
```bash
# Ensure vendored OpenSSL
export OPENSSL_STATIC=1
cargo clean
cargo build --release
```

### Android Build Fails

**Symptom:** `cargo ndk` errors or missing NDK

**Solution:**
1. Verify NDK path in `android/local.properties`
2. Ensure NDK v28.2.13676358 is installed
3. Check `ANDROID_HOME` environment variable

### iOS Build Fails

**Symptom:** CocoaPods errors or missing pods

**Solution:**
```bash
cd ios
pod install --repo-update
cd ..
flutter clean
flutter pub get
```

### Flutter Build Fails

**Symptom:** Swift Package Manager warnings

**Solution:**
Ensure `pubspec.yaml` has:
```yaml
flutter:
  config:
    enable-swift-package-manager: false
```

## Runtime Issues

### Engine Fails to Start

**Symptom:** "Engine failed on existing DB" in logs

**Solution:**
1. Delete corrupted database:
   ```bash
   rm -f introvert.db
   ```
2. Restart the app

### No Network Connection

**Symptom:** Status shows "OFFLINE" permanently

**Solution:**
1. Check internet connectivity
2. Verify RBN is reachable:
   ```bash
   nc -z -w 5 47.89.252.80 443
   ```
3. Check firewall settings (Port 443 open)
4. Try WebSocket tunnel mode

### Messages Not Delivering

**Symptom:** Messages stuck at "Sent" status

**Solution:**
1. Check peer connectivity
2. Verify recipient is online
3. Check RBN mailbox sync:
   ```bash
   journalctl -u introvertd -n 50
   ```
4. Try reconnecting

### File Transfer Fails

**Symptom:** File chunks not received

**Solution:**
1. Check file size (must be <1GB)
2. Verify both peers are connected
3. Check disk space on recipient
4. Try smaller test file

## Network Issues

### Cannot Connect to RBN

**Symptom:** Connection timeout to 47.89.252.80:443

**Solution:**
1. Check firewall:
   ```bash
   ufw allow 443
   ```
2. Verify DNS resolution
3. Try different network
4. Check RBN status:
   ```bash
   systemctl status introvertd
   ```

### Relay Connection Slow

**Symptom:** Low throughput on relayed connections

**Solution:**
1. Expected behavior (0.3-1 Mbps)
2. Try direct connection (same network)
3. Check RBN load
4. Verify no network congestion

### DHT Lookup Slow

**Symptom:** Handle resolution takes >5 seconds

**Solution:**
1. Check DHT routing table
2. Verify bootstrap nodes are reachable
3. Check network latency
4. Try again (DHT may be updating)

## iOS/macOS Issues

### App Crashes on Launch

**Symptom:** Immediate crash after splash screen

**Solution:**
1. Check sandbox path resolution
2. Verify entitlements in Xcode
3. Clean build folder
4. Reinstall app

### Local Network Not Working

**Symptom:** Cannot discover peers on same Wi-Fi

**Solution:**
1. Check `NSLocalNetworkUsageDescription` in Info.plist
2. Verify `NSBonjourServices` includes `_ipfs._udp`
3. Check local network permissions in Settings
4. Restart app after permission grant

### Background Service Stops

**Symptom:** Calls not received when app is backgrounded

**Solution:**
1. Check background modes in Xcode
2. Verify VoIP entitlements
3. Check notification permissions
4. Test with `flutter_callkit_incoming`

## Intro-Claw Issues

### Intro-Claw Engine Not Running

**Symptom:** CLAW tab shows "Engine inactive" or tick loop not firing

**Solution:**
1. Check `intro_claw_active` key in `economy_meta` table:
   ```sql
   SELECT value FROM economy_meta WHERE key = 'intro_claw_active';
   ```
2. Ensure the value is `true`
3. Restart the app after enabling

### Semantic Intent Engine Slow or Fallback

**Symptom:** Queries return keyword-matched results instead of semantic matches

**Solution:**
1. Verify `candle-core` dependency is compiled (check build output)
2. Model download may be in progress — wait for first use
3. Keyword fallback is normal when model is not loaded; check logs for model load errors

### Hybrid AI Mode Not Responding

**Symptom:** CLAW queries return "AI mode unavailable"

**Solution:**
1. Verify `intro_claw_ai_mode` is enabled:
   ```sql
   SELECT value FROM economy_meta WHERE key = 'intro_claw_ai_mode';
   ```
2. Check `intro_claw_endpoint` is set:
   ```sql
   SELECT value FROM economy_meta WHERE key = 'intro_claw_endpoint';
   ```
3. Verify API key is valid (encrypted in `intro_claw_api_key`)
4. Test endpoint connectivity: `curl -s <endpoint>/v1/models`

### Network Recon Failing

**Symptom:** Diagnostic reports show incomplete or empty data

**Solution:**
1. Ensure the engine is online (not in Offline mode)
2. Check peer connectivity — recon requires at least one active connection
3. Try manual recon via FFI: `intro_claw_run_network_recon()`

### Peer Healing Not Working

**Symptom:** `intro_claw_heal_peer` returns failure

**Solution:**
1. Check which strategy was attempted (direct dial, relay, anchor, WebSocket, mailbox)
2. Verify the peer is reachable through at least one path
3. Check RBN status if relay/anchor strategies were used
4. Review logs for the specific failure point

## Performance Issues

### High Memory Usage

**Symptom:** App uses >500MB RAM

**Solution:**
1. Close other apps
2. Check for memory leaks in file transfers
3. Restart app
4. Report issue with logs

### Slow File Transfer

**Symptom:** Transfer speed <100 Kbps

**Solution:**
1. Check connection type (direct vs relayed)
2. Verify no network throttling
3. Try smaller files
4. Check disk I/O speed

### Battery Drain

**Symptom:** High battery usage in background

**Solution:**
1. Check background service settings
2. Verify polling intervals
3. Use WiFi when possible
4. Report issue with battery stats

## Logging

### Enable Debug Logs
```bash
# Flutter
flutter run --verbose

# Rust
RUST_LOG=debug ./introvertd

# Android
adb logcat | grep introvert
```

### Collect Logs
```bash
# Flutter
flutter logs

# Android
adb logcat -d > android_logs.txt

# macOS
log show --predicate 'process == "introvert"' > macos_logs.txt
```

## Getting Help

### Before Asking
1. Check this troubleshooting guide
2. Search existing GitHub issues
3. Check `Docs/` directory

### When Asking
Include:
- Platform and OS version
- Steps to reproduce
- Error messages/logs
- Screenshots if applicable

### Resources
- GitHub Issues: [link]
- Discord: [link]
- Documentation: `Docs/` directory
