# Rectification Plan — PR-1.5: Platform mDNS Permissions

**Date:** 2026-07-18
**Status:** PENDING EXPERT APPROVAL
**Blocking:** PR-2 (seeder cascade) depends on this
**Risk:** Low — platform config changes only, no Rust/Dart code changes

---

## 1. Problem Summary

PR-1 merged successfully (commit `4a55fde`). On-device validation showed:
- Drain cooldowns and in-flight cap working correctly
- File transfers fast via relay path
- **Zero mDNS peer discovery** across all 3 platforms

The `TransferRouter`'s Direct path (P1) and LocalSeeder path (P2/P3) are never triggered because `mdns_peers` is empty. The expert confirmed this disables the entire P1–P3 priority chain, not just P1.

**Root cause:** OS-level permission gap — Android and macOS are missing required permissions for mDNS multicast. The libp2p mDNS behaviour initializes successfully but the OS drops inbound multicast packets before they reach the socket.

---

## 2. OS Permission Audit

| Platform | Permission/Config | File | Status |
|----------|-------------------|------|--------|
| iOS | `NSLocalNetworkUsageDescription` | `ios/Runner/Info.plist` | PRESENT |
| iOS | `NSBonjourServices` | `ios/Runner/Info.plist` | PRESENT |
| Android | `CHANGE_WIFI_MULTICAST_STATE` | `android/app/src/main/AndroidManifest.xml` | **MISSING** |
| Android | `MulticastLock` acquire/release | Native Kotlin code | **MISSING** |
| macOS | `NSLocalNetworkUsageDescription` | `macos/Runner/Info.plist` | **MISSING** |
| macOS | `NSBonjourServices` | `macos/Runner/Info.plist` | **MISSING** |
| macOS | `com.apple.security.network.multicast` | `macos/Runner/Release.entitlements` | **MISSING** |
| macOS | `com.apple.security.network.multicast` | `macos/Runner/DebugProfile.entitlements` | **MISSING** |

---

## 3. Fix Details

### 3.1 Android — `CHANGE_WIFI_MULTICAST_STATE` + `MulticastLock`

**Why:** Android's WiFi stack drops inbound multicast packets by default as a power-saving measure. The app must explicitly acquire a `MulticastLock` to receive mDNS responses (UDP 224.0.0.251:5353). Without this, libp2p's mDNS socket never receives any packets.

**File: `android/app/src/main/AndroidManifest.xml`**
Add inside `<manifest>`:
```xml
<uses-permission android:name="android.permission.CHANGE_WIFI_MULTICAST_STATE" />
```

**File: `android/app/src/main/kotlin/com/example/introvert/MainActivity.kt`**
Add MulticastLock management:
```kotlin
import android.net.wifi.WifiManager
import android.content.Context

class MainActivity : FlutterActivity() {
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        acquireMulticastLock()
    }

    private fun acquireMulticastLock() {
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("introvert_mdns").apply {
            setReferenceCounted(true)
            acquire()
        }
    }

    override fun onDestroy() {
        multicastLock?.release()
        multicastLock = null
        super.onDestroy()
    }
}
```

**Behavior:** Lock is acquired when the Flutter engine starts, released when the activity is destroyed. `setReferenceCounted(true)` means multiple acquire/release calls are balanced automatically.

### 3.2 macOS — Entitlements + Info.plist

**Why:** macOS sandboxed apps (release builds) cannot send/receive multicast traffic without the `com.apple.security.network.multicast` entitlement. Without `NSBonjourServices`, the OS may not pre-approve the Bonjour service types for discovery.

**File: `macos/Runner/Release.entitlements`**
Add:
```xml
<key>com.apple.security.network.multicast</key>
<true/>
```

**File: `macos/Runner/DebugProfile.entitlements`**
Add:
```xml
<key>com.apple.security.network.multicast</key>
<true/>
```

**File: `macos/Runner/Info.plist`**
Add before `</dict>`:
```xml
<key>NSLocalNetworkUsageDescription</key>
<string>Introvert uses the local network to discover and connect to other Introvert nodes on your Wi-Fi for fast peer-to-peer file transfers.</string>
<key>NSBonjourServices</key>
<array>
    <string>_ipfs-discovery._udp</string>
    <string>_ipfs._udp</string>
    <string>_libp2p._udp</string>
</array>
```

### 3.3 iOS — Already Configured (Verification Only)

`NSLocalNetworkUsageDescription` and `NSBonjourServices` are already present in `ios/Runner/Info.plist`. If mDNS still fails on iOS after this PR, check:
- Settings → Introvert → Local Network toggle — must be ON
- If the toggle is missing, the user may have denied the prompt on first launch — requires app reinstall or Settings reset

---

## 4. Verification Plan

After applying PR-1.5:

1. **Android:** Rebuild and run. Check logcat for `mDNS discovered peer` lines. Should see the other 2 devices' peer IDs.

2. **macOS:** Rebuild (release config) and run. Check logs for `mDNS discovered peer` lines. Should see the other 2 devices' peer IDs.

3. **iOS:** If already working, no change needed. If not, check Settings → Introvert → Local Network.

4. **Cross-check:** All 3 devices should show `mDNS discovered peer: <peer_id> with N addresses` in their logs within 30 seconds of startup.

5. **TransferRouter activation:** After mDNS is working, the next file transfer should show `[Mesh] TransferRouter: Direct for <transfer_id> — sending direct to <peer_id>` in the logs for LAN peers.

6. **Throughput measurement:** Transfer a 10MB file and measure transfer_size / time. Compare to:
   - Direct path target: 70+ Mbps (LAN speed)
   - Relay path baseline: current "fast" speed (unmeasured)

---

## 5. Sequencing

| Step | Change | Risk | Dependency |
|------|--------|------|------------|
| PR-1.5 | Android manifest + MulticastLock + macOS entitlements + Info.plist | Low | None |
| Re-test | Verify mDNS discovery on all 3 platforms | — | PR-1.5 merged |
| PR-2 | Seeder cascade + pull-only chunks | Medium | mDNS confirmed working |

**Do not ship PR-2 until mDNS discovery is confirmed working** — the seeder cascade depends on `mdns_peers` being populated.

---

## 6. Fallback Mechanism (Deferred)

The expert suggested a candidate-address-exchange fallback (peers exchange local addresses via RBN signaling, then attempt direct dial). This is defense-in-depth for cases where mDNS still fails after permissions are fixed (e.g., router client isolation).

**Recommendation:** Defer to a future PR. If PR-1.5 fixes mDNS on all 3 platforms, the fallback is unnecessary. If mDNS still fails on some devices after PR-1.5, implement the fallback then.

---

## 7. Files to Modify

| File | Change |
|------|--------|
| `android/app/src/main/AndroidManifest.xml` | Add `CHANGE_WIFI_MULTICAST_STATE` permission |
| `android/app/src/main/kotlin/com/example/introvert/MainActivity.kt` | Add MulticastLock acquire/release |
| `macos/Runner/Release.entitlements` | Add `com.apple.security.network.multicast` |
| `macos/Runner/DebugProfile.entitlements` | Add `com.apple.security.network.multicast` |
| `macos/Runner/Info.plist` | Add `NSLocalNetworkUsageDescription` + `NSBonjourServices` |

---

## 8. Expert Review Feedback (2026-07-18)

### 8.1 iOS Contradiction — RESOLVED

**Finding:** `NSBonjourServices` lists `_ipfs-discovery._udp`, `_ipfs._udp`, `_libp2p._udp` but libp2p-mdns 0.48.0 uses `_p2p._udp` as its service name (confirmed from crate source: `const SERVICE_NAME: &[u8] = b"_p2p._udp.local"`).

**Impact:** If iOS uses `NSBonjourServices` to filter raw multicast traffic, the mismatched service names would cause the OS to drop mDNS packets even though the permission is granted.

**Fix:** Update `NSBonjourServices` in `ios/Runner/Info.plist` to include `_p2p._udp` (the actual service name used by libp2p-mdns). Keep the existing entries for backward compatibility.

### 8.2 MulticastLock Scope — REVISED

**Expert concern:** `MulticastLock` tied to `MainActivity.onDestroy()` — lock released on backgrounding-with-recreation, not just app kill. The Rust core needs mDNS while backgrounded.

**Revised approach:** Move MulticastLock management to the Flutter plugin layer or a persistent `Service` component, not `MainActivity`. The lock should be acquired when the network starts (FFI `network_start`) and released on `network_stop` or app termination.

**Alternative:** If the app already has a foreground service for networking (referenced in DEBUG_DOCUMENT.md), attach the MulticastLock to that service's lifecycle instead of the Activity.

### 8.3 Permission-Reset Requirement — GENERALIZED

All three platforms have sticky Local Network permissions:
- **iOS:** Settings → [app] → Local Network toggle (requires reinstall to reset)
- **macOS:** System Settings → Network → Local Network (per-app, sticky)
- **Android:** No per-app Local Network permission prompt, but MulticastLock is app-managed

Verification plan should note this for all platforms, not just iOS.

### 8.4 NSBonjourServices Verification — CONFIRMED

libp2p-mdns 0.48.0 uses `_p2p._udp` (from `Cargo.lock` and crate source). The existing `NSBonjourServices` entries (`_ipfs-discovery._udp`, `_ipfs._udp`, `_libp2p._udp`) do NOT match. Add `_p2p._udp` to the array.

---

## 9. Revised Verification Plan

Before merging PR-1.5:

1. **iOS (5-minute check):** On the already-tested device, check Settings → Introvert → Local Network. Is the toggle present? Is it on? This determines whether the iOS failure is a permission denial or something else.

2. **After PR-1.5 merge:** Rebuild all 3 platforms and verify `mDNS discovered peer` appears in logs within 30 seconds of startup.

3. **Permission reset:** If any platform still shows zero discovery after rebuild, note that the user may need to:
   - iOS: Reinstall app or reset Local Network permissions
   - macOS: Remove and re-add the app in System Settings → Network → Local Network
   - Android: No reset needed (MulticastLock is app-managed)

4. **Throughput measurement:** Transfer a 10MB file, measure transfer_size / time, compare to 70+ Mbps LAN target.

---

## 10. Expert Review — Final Corrections (2026-07-18)

### 10.1 §8.1 Correction — NSBonjourServices Does NOT Gate libp2p mDNS

`NSBonjourServices` governs Apple's `NWBrowser`/`NetServiceBrowser` APIs. libp2p-mdns does NOT use those APIs — it opens its own raw UDP multicast socket to `224.0.0.251:5353` and speaks DNS-SD wire format directly. The iOS Local Network permission gate is triggered by any local-subnet/multicast traffic attempt, not scoped to service names.

**Impact:** Adding `_p2p._udp` to `NSBonjourServices` is harmless but **not a fix**. The iOS contradiction (permissions present, zero discovery) is **still unexplained**.

**Action:** Run Settings → Introvert → Local Network toggle check BEFORE merging PR-1.5. If toggle is on and discovery is still zero, the permission hypothesis is falsified for iOS — escalate to packet capture on the iOS device.

### 10.2 §8.2 — MulticastLock Scope Needs Concrete Answer

**Open question:** Is the app's persistent networking component a foreground `Service` (per DEBUG_DOCUMENT.md's `startForegroundCompat()`/`FOREGROUND_SERVICE_TYPE_SPECIAL_USE`) or Activity-scoped?

**Action:** Confirm this before implementing. If Activity-scoped, move MulticastLock to the foreground Service or Flutter plugin layer.

### 10.3 §8.3 — macOS Entitlement Is Restricted

`com.apple.security.network.multicast` requires Apple request/approval for App Store builds. Not blocking for dev/TestFlight, but track as separate parallel item.

### 10.4 Revised Verdict
- **Android (§3.1):** Approved outright
- **macOS (§3.2):** Approved for dev/test builds. Track App Store entitlement separately.
- **iOS (§3.3/§8.1):** Keep `_p2p._udp` addition. Run Settings toggle check FIRST. If toggle is on + zero discovery, escalate to packet capture.

### 10.5 Pre-Merge Gate
Run the iOS Settings toggle check before merging anything. The result determines next steps:
- **Toggle off or missing:** Permission was denied. Reinstall app → toggle appears → mDNS should work. Merge PR-1.5 as-is.
- **Toggle on, zero discovery:** Permission hypothesis falsified for iOS. Do NOT merge PR-1.5 as "the fix" — investigate raw-socket path (packet capture).
