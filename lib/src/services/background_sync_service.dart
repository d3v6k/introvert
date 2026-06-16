import 'dart:async';
import 'package:flutter/foundation.dart';
import '../native/introvert_client.dart';

/// Background sync service using Timer.periodic for mailbox polling.
/// 
/// Uses a 5-minute interval for mailbox fetching.
/// This is the fallback approach when WorkManager is not available.
class BackgroundSyncService {
  static BackgroundSyncService? _instance;
  static BackgroundSyncService get instance => _instance ??= BackgroundSyncService._();
  BackgroundSyncService._();

  Timer? _fallbackTimer;
  bool _initialized = false;

  /// Initialize background sync with periodic timer.
  Future<void> initialize() async {
    if (_initialized) return;
    _initialized = true;

    _fallbackTimer?.cancel();
    _fallbackTimer = Timer.periodic(const Duration(minutes: 5), (timer) {
      try {
        IntrovertClient().fetchMailbox();
        debugPrint("📬 Background: Mailbox fetched");
      } catch (e) {
        debugPrint("❌ Background mailbox fetch failed: $e");
      }
    });
    debugPrint("✅ Background sync initialized (5 min interval)");
  }

  /// Cancel all background tasks.
  void cancel() {
    _fallbackTimer?.cancel();
  }

  void dispose() {
    cancel();
  }
}
