# Rectification Plan: Android Stability and Cross-Network File Transfer Fixes

**Date:** 2026-07-13  
**Authors:** Antigravity  
**Target Audience:** Mimo CLI / Development Team  
**Scope:** Android Native (Kotlin), Rust FFI Core, Swarm Network Module, and Gossipsub Routing  

---

## Executive Summary

A full ground-up audit of the **Introvert Sovereign Messenger** has identified the root causes for:
1. **Android App Instability**: Under API 34+ (Android 14), starting a foreground service without specifying a service type throws an unhandled crash. Furthermore, background starts from FCM wakeups trigger FGS start restrictions, while Rust FFI panic unwinding leads to raw JNI segfaults. Battery optimization prompts also violate Google Play policies.
2. **Cross-Network File Transfer Failures**: Relayed peers using Gossipsub fallback hit a subscription deadlock. Neither sender nor receiver is subscribed to the file-transfer topics prior to transmission. Additionally, 1:1 file transfers broadcast all binary chunks (64KB/256KB) to the entire network via the `"file-transfer-global"` topic, leading to extreme bandwidth waste and OOM crashes.

This document details the step-by-step technical rectification plan to resolve these issues.

---

## Part 1: Android Stability (Native & FFI Boundary)

### [AND-1] Specify Foreground Service Type on API 34+
*   **Target File:** `android/app/src/main/kotlin/chat/introvert/app/IntrovertService.kt`
*   **Problem:** Android 14 (API 34) requires that `startForeground` specifies the foreground service type when the target SDK is 34+. Calling `startForeground(NOTIFICATION_ID, builder.build())` throws `MissingForegroundServiceTypeException` and crashes the process.
*   **Solution:** Use the Android Q+ `startForeground` overload and pass `ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE`.
*   **Kotlin Change:**
    ```kotlin
    // Replace L89 in IntrovertService.kt:
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        startForeground(
            NOTIFICATION_ID, 
            builder.build(), 
            android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        )
    } else {
        startForeground(NOTIFICATION_ID, builder.build())
    }
    ```

### [AND-2] Catch Background FGS Start Restrictions Exception
*   **Target File:** `android/app/src/main/kotlin/chat/introvert/app/IntrovertFirebaseMessagingService.kt`
*   **Problem:** Starting a foreground service from the background (e.g. upon receiving an FCM push in the background) is highly restricted in Android 12+. It throws `ForegroundServiceStartNotAllowedException` (which inherits from `IllegalStateException`), crashing the app.
*   **Solution:** Catch this exception specifically in `wakeForegroundService()` to prevent crashes, logging the warning and falling back to a deferred job execution or letting the message fetch wait until user focus.
*   **Kotlin Change:**
    ```kotlin
    // Replace wakeForegroundService() in IntrovertFirebaseMessagingService.kt:
    private fun wakeForegroundService() {
        try {
            val intent = Intent(this, IntrovertService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(intent)
            } else {
                startService(intent)
            }
            Log.d(TAG, "Foreground service started for message fetch")
        } catch (e: IllegalStateException) {
            Log.e(TAG, "FGS start not allowed from background (Android restrictions): ${e.message}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start foreground service: ${e.message}")
        }
    }
    ```

### [AND-3] Remove Ignored Battery Optimization Prompts
*   **Target File:** `android/app/src/main/kotlin/chat/introvert/app/MainActivity.kt`
*   **Problem:** Proactively launching `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` at startup violates Google Play policies for standard messaging apps, leading to rejection.
*   **Solution:** Remove `requestBatteryOptimizationExemption()` from `onCreate` initialization. Keep the method block or remove it completely to ensure policy compliance.
*   **Kotlin Change:**
    ```kotlin
    // Remove line 43 or comment it out in MainActivity.kt:
    // requestBatteryOptimizationExemption()
    ```

### [AND-4] Wrap FFI Boundary in Panic Catch Guards
*   **Target File:** `src/lib.rs`
*   **Problem:** Standard C-ABI FFI entry points do not catch panics. If Rust code hits a panic (e.g. from `.unwrap()` on None/Err, array bounds, etc.), it unwinds across the FFI, leading to undefined behavior and immediate JVM crash on Android.
*   **Solution:** Wrap all critical FFI exports in `std::panic::catch_unwind` and return an `FfiResult::error()` code.
*   **Rust Example:**
    ```rust
    // Pattern to apply to all FFI functions in src/lib.rs:
    #[no_mangle]
    pub extern "C" fn introvert_network_some_action(...) -> FfiResult {
        std::panic::catch_unwind(|| {
            // Actual function body logic...
        }).unwrap_or_else(|_| {
            FfiResult::error(-99, "Rust panicked at FFI boundary")
        })
    }
    ```

### [AND-5] Remove Risky Unwraps in Swarm Event Handling
*   **Target File:** `src/network/mod.rs`
*   **Problem:** Line 2927: `let group = self.storage.get_group(topic_str).ok().flatten().unwrap();` will panic if the user deletes a group chat exactly when a message is propagating.
*   **Solution:** Replace the unwrap with safe match handling.
*   **Rust Change:**
    ```rust
    // Replace L2927 in src/network/mod.rs:
    let group = match self.storage.get_group(topic_str).ok().flatten() {
        Some(g) => g,
        None => {
            warn!("[Mesh] Group {} not found in DB during membership verification", topic_str);
            return Ok(());
        }
    };
    ```

---

## Part 2: Cross-Network File Transfer (Gossipsub Fallback Isolation)

### [NET-1] Unique Topic Isolation (`file-transfer-{transfer_id}`)
*   **Target File:** `src/network/mod.rs`
*   **Problem:** 1:1 file transfers fall back to `"file-transfer-global"` when `group_id` is `None`. This broadcasts all chunks (87KB base64 encoded) to the entire network, causing massive amplification, network congestion, and Android OOM crashes. Group transfers also broadcast chunks to all members of the group chat regardless of whether they are downloading the file.
*   **Solution:** Route all file chunk and request traffic through a topic unique to the transfer session: `file-transfer-{transfer_id}`.
*   **Rust Change in `forward_to_mesh` (L3934-3941):**
    ```rust
    if is_file_payload && is_relayed_for_send {
        // Unique file transfer topic isolation: file-transfer-{transfer_id}
        // Prevents broadcasting chunks to the entire group or global channel.
        let transfer_id = match &payload {
            SignalingPayload::FileChunk { transfer_id, .. } => transfer_id.clone(),
            SignalingPayload::FileChunkRequest { transfer_id, .. } => transfer_id.clone(),
            _ => "global".to_string(),
        };
        let topic_str = format!("file-transfer-{}", transfer_id);
        let topic = libp2p::gossipsub::IdentTopic::new(&topic_str);
        
        // Subscribe to ensure we can receive responses/acknowledgements
        let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&topic);
        if let Ok(bytes) = serde_json::to_vec(&payload) {
            match self.swarm.behaviour_mut().gossipsub.publish(topic.clone(), bytes) {
                Ok(_) => {
                    info!("[Mesh] Published {} via gossipsub topic={} (isolated fallback)", payload_desc, topic_str);
                    self.mark_group_action_sent(recipient_id, &payload);
                }
                Err(e) => {
                    warn!("[Mesh] Gossipsub publish FAILED for {}: {:?}", payload_desc, e);
                }
            }
        }
    }
    ```
*   **Rust Change in `Stall retry: inline flush` (L660-666):**
    ```rust
    if is_relayed && matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
        let transfer_id = match &payload {
            SignalingPayload::FileChunk { transfer_id, .. } => transfer_id.clone(),
            SignalingPayload::FileChunkRequest { transfer_id, .. } => transfer_id.clone(),
            _ => "global".to_string(),
        };
        let topic_str = format!("file-transfer-{}", transfer_id);
        let topic = libp2p::gossipsub::IdentTopic::new(&topic_str);
        let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&topic);
        if let Ok(bytes) = serde_json::to_vec(&payload) {
            let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, bytes);
        }
    }
    ```

### [NET-2] Proactive Gossipsub Subscription Flow
*   **Target File:** `src/network/mod.rs`
*   **Problem:** Peers only subscribe to topics when calling `forward_to_mesh` to *send* a payload. The sender is not subscribed to `file-transfer-{transfer_id}` when the receiver starts downloading, so the receiver's initial `FileChunkRequest` is lost.
*   **Solution:** Force both sender and receiver to proactively subscribe to the unique transfer topic when the transfer is initialized.
*   **Rust Change (Receiver Side - L7460 in `FileTransfer` handler):**
    ```rust
    // Right after self.incoming_transfers.insert(...)
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", transfer_id));
    let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&ft_topic);
    crate::dispatch_debug_log(&format!("[CMD] Gossipsub: Subscribed to file-transfer-{} (receiver)", transfer_id));
    ```
*   **Rust Change (Sender Side - L4707 in `RegisterSeeder` command):**
    ```rust
    // Right after self.active_seeders.insert(...)
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", transfer_id));
    let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&ft_topic);
    crate::dispatch_debug_log(&format!("[CMD] Gossipsub: Subscribed to file-transfer-{} (seeder)", transfer_id));
    ```

### [NET-3] Cleanup Unsubscriptions (Prevent Resource Leak)
*   **Target File:** `src/network/mod.rs`
*   **Problem:** Subscription lists grow monotonically because nodes never unsubscribe from the file transfer topics once a file transfer concludes.
*   **Solution:** Call `gossipsub.unsubscribe(&topic)` on transfer completion, cancellation, and eviction.
*   **Rust Change (Receiver Success - L1772):**
    ```rust
    if is_complete {
        crate::ACTIVE_PULLS.lock().remove(&transfer_id);
        self.incoming_transfers.remove(&transfer_id);
        self.active_downloads.remove(&transfer_id);

        // Unsubscribe from file-transfer-{transfer_id} topic
        let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", transfer_id));
        let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&ft_topic);
        crate::dispatch_debug_log(&format!("[CMD] Gossipsub: Unsubscribed from file-transfer-{} (complete)", transfer_id));

        self.service_queued_downloads();
    }
    ```
*   **Rust Change (Receiver Cancel - L4746):**
    ```rust
    self.incoming_transfers.remove(&transfer_id);
    self.active_downloads.remove(&transfer_id);
    
    // Unsubscribe from file-transfer-{transfer_id} topic
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", transfer_id));
    let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&ft_topic);
    crate::dispatch_debug_log(&format!("[CMD] Gossipsub: Unsubscribed from file-transfer-{} (canceled)", transfer_id));
    ```
*   **Rust Change (Stale Eviction - L735):**
    ```rust
    self.incoming_transfers.remove(id);
    self.active_downloads.remove(id);
    
    // Unsubscribe from file-transfer-{id} topic
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", id));
    let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&ft_topic);
    crate::dispatch_debug_log(&format!("[CMD] Gossipsub: Unsubscribed from file-transfer-{} (stale)", id));
    ```

---

## Verification Plan

### 1. Android Stability Verification
*   **FGS Type Test**: Launch the app on a physical Android 14 device. Verify the background service starts up without throwing `MissingForegroundServiceTypeException`.
*   **Background Wakeup Test**: Put the app in the background and lock the screen. Send a FCM push notification to the device. Verify that:
    1. The native notification banner appears.
    2. The app does not crash in the background (no `ForegroundServiceStartNotAllowedException` in `adb logcat`).
*   **Google Play Compliance Check**: Confirm that battery optimization dialog does not show up on app start.

### 2. Isolated Cross-Network File Transfer Verification
*   **1:1 Transfer Speed**: Establish a relay circuit between Android (behind VPN) and Mac (on local WiFi). Send a 10MB file. Verify that transfer progress moves from 0% immediately and completes successfully.
*   **No Global Traffic Leak**: Connect a third device (iOS) to the RBN mesh. Perform a file transfer between Android and Mac. Inspect the iOS device's logs during the transfer. Verify that the iOS device receives **zero** file chunks (no `gossipsub.Message` events for the active transfer ID on iOS).
*   **Subscription Cleanup**: Monitor Gossipsub topic subscriptions using swarm diagnostics FFI or logs. Verify that the number of active topics drops back to the baseline group count after a file transfer completes or is cancelled.
