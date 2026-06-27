import 'dart:async';
import 'package:flutter/foundation.dart';
import '../native/introvert_client.dart';

/// Background sync service — FCM push replaces polling when available.
///
/// When FCM is configured, the app only wakes on push notifications.
/// The periodic mailbox poll is kept as a fallback for devices without FCM
/// or when push delivery fails.
class BackgroundSyncService {
  static BackgroundSyncService? _instance;
  static BackgroundSyncService get instance => _instance ??= BackgroundSyncService._();
  BackgroundSyncService._();

  static const Duration _pollInterval = Duration(minutes: 2);

  Timer? _fallbackTimer;
  bool _initialized = false;
  bool _isIdle = false;
  bool _pushAvailable = false;

  /// Initialize background sync.
  /// [pushAvailable] should be true if FCM/APNs is configured and working.
  Future<void> initialize({bool pushAvailable = true}) async {
    if (_initialized) return;
    _initialized = true;
    _pushAvailable = pushAvailable;

    if (pushAvailable) {
      debugPrint("✅ Background sync initialized — push active, polling disabled");
    } else {
      debugPrint("⚠️ Background sync initialized — push unavailable, starting fallback poll");
      _startFallbackPoll();
    }
  }

  /// Start periodic mailbox polling as a fallback when push is unavailable.
  void _startFallbackPoll() {
    _fallbackTimer?.cancel();
    _fallbackTimer = Timer.periodic(_pollInterval, (_) {
      if (!_isIdle) {
        debugPrint("[BackgroundSync] Fallback poll — fetching mailbox");
        IntrovertClient().fetchMailbox();
      }
    });
  }

  /// Enter idle mode: disable all non-essential background activity.
  /// Push handles all wake-ups. Only Kademlia/Gossipsub internals remain active.
  void enterIdleMode() {
    if (_isIdle) return;
    _isIdle = true;
    _fallbackTimer?.cancel();
    debugPrint("[IdleMode] Entered idle — push handles wake-ups, polling disabled");
  }

  /// Exit idle mode: resume normal background activity.
  void exitIdleMode() {
    if (!_isIdle) return;
    _isIdle = false;
    debugPrint("[IdleMode] Exited idle — resuming normal activity");

    // If push is unavailable, restart fallback polling
    if (!_pushAvailable) {
      _startFallbackPoll();
    }
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
