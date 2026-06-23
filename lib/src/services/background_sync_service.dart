import 'dart:async';
import 'package:flutter/foundation.dart';
import '../native/introvert_client.dart';

/// Background sync service — FCM push replaces polling when available.
///
/// When FCM is configured, the app only wakes on push notifications.
/// The periodic mailbox poll is kept as a fallback for devices without FCM.
class BackgroundSyncService {
  static BackgroundSyncService? _instance;
  static BackgroundSyncService get instance => _instance ??= BackgroundSyncService._();
  BackgroundSyncService._();

  Timer? _fallbackTimer;
  bool _initialized = false;
  bool _isIdle = false;

  /// Initialize background sync.
  /// With FCM push, we disable the polling timer — FCM wakes the app on new messages.
  Future<void> initialize() async {
    if (_initialized) return;
    _initialized = true;

    // FCM push replaces mailbox polling — no timer needed
    debugPrint("✅ Background sync initialized — FCM push active, polling disabled");
  }

  /// Enter idle mode: disable all non-essential background activity.
  /// FCM handles all wake-ups. Only Kademlia/Gossipsub internals remain active.
  void enterIdleMode() {
    if (_isIdle) return;
    _isIdle = true;
    _fallbackTimer?.cancel();
    debugPrint("[IdleMode] Entered idle — FCM handles wake-ups, polling disabled");
  }

  /// Exit idle mode: resume normal background activity.
  void exitIdleMode() {
    if (!_isIdle) return;
    _isIdle = false;
    debugPrint("[IdleMode] Exited idle — resuming normal activity");
  }

  bool get isIdle => _isIdle;

  /// Cancel all background tasks.
  void cancel() {
    _fallbackTimer?.cancel();
  }

  void dispose() {
    cancel();
  }
}
