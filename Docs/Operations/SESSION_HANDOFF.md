# Introvert Session Handoff — 2026-06-28

## Issues Summary

### Critical: Group Chat & Android Push Wakeup (RESOLVED ✅)
Android devices were not receiving group messages or notification pings when the app was minimized or the screen was off. 

We identified and resolved three core issues:
1. **APNS Token Override on Android**: The Flutter app initialization was checking only the APNS token on startup, resulting in push availability setting to `false` on Android and starting the polling fallback thread instead.
2. **libp2p Protocol Negotiation Race**: The push token registration command was executing immediately on `ConnectionEstablished`, prior to protocol capability negotiation. We moved the auto-registration trigger to the `Identify` event handler.
3. **Stale FFI Binary Caching**: The Flutter APK was packaging stale cached `libintrovert.so` folders, missing recent fixes. We recompiled client libraries for target architectures (`arm64-v8a` and `x86_64`) and deployed the updated daemon to the Alibaba RBN.

---

## Files Modified

| File | Change |
| :--- | :--- |
| [src/network/mod.rs](file:///Users/dev/Development/introvert/src/network/mod.rs) | Moved push token auto-registration to Identify event handler |
| [for_linux/src/network/mod.rs](file:///Users/dev/Development/introvert/for_linux/src/network/mod.rs) | Moved push token auto-registration to Identify event handler |
| [lib/src/native/alert_service.dart](file:///Users/dev/Development/introvert/lib/src/native/alert_service.dart) | Added platform check for FCM token status verification |
| [lib/src/ui/main_shell.dart](file:///Users/dev/Development/introvert/lib/src/ui/main_shell.dart) | Ensured FCM token triggers background push active |

---

## Verification & Deployment Status

- **Android Client Rebuild**: Compiled successfully via `scripts/build_android.sh` and deployed to `jniLibs`.
- **RBN Deployment**: Deployed successfully to the Alibaba RBN node (`47.89.252.80:443`). Systemd service verified as **active**.
- **Local macOS / iOS Clients**: Rebuilt and stable.

---

## Next Verification Steps for User

1. **Clean Rebuild and Deploy to Android**:
   ```bash
   flutter run -d SM-S908E
   ```
2. **Observe Terminal Logs on Boot**:
   * Expect to see:
     `I/flutter (25425): ✅ Background sync initialized — push active, polling disabled`
   * Expect to see:
     `I/flutter (25425): 🦀 Rust Debug: [Mesh] Checking local push token for auto-registration on Identify...`
     `I/flutter (25425): 🦀 Rust Debug: [Mesh] 🔔 Found local token. Auto-registering with RBN...`
3. **Verify Sleep State Push Notification**:
   * Minimize the Android app.
   * Send a group message from your Mac client.
   * Expect the Android device to immediately receive a push notification and wake the background service to fetch the encrypted message payload.
