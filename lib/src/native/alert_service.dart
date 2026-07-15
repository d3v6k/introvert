import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'introvert_client.dart';
import '../services/background_sync_service.dart';

/// Thin Dart wrapper around the native `introvert/alerts` MethodChannel.
///
/// Responsibilities:
/// - Request notification permissions on first use (Android 13+ / iOS).
/// - Route `showAlert` calls to the native side which posts a
///   `UNUserNotificationCenter` notification (iOS) or a
///   `NotificationCompat` notification (Android) and plays the Introvert ping.
/// - Enforce 3-minute global cooldown on phone notifications.
/// - Suppress notifications when app is in foreground (sound only).
class AlertService {
  static const MethodChannel _channel = MethodChannel('introvert/alerts');

  static bool _permissionsRequested = false;
  static String? _apnsToken;
  static String? _pendingFcmToken;
  static bool _hasRegisteredToken = false;

  // ── Notification cooldown (3 minutes) ─────────────────────────────────────
  static DateTime? _lastNotificationTime;
  static const Duration _notificationCooldown = Duration(minutes: 3);

  // ── Foreground state (set by main_shell.dart) ─────────────────────────────
  static bool _isInForeground = false;

  // ── Wakeup cooldown (30s) — breaks the FCM echo loop ─────────────────────
  static DateTime? _lastWakeupTime;
  static const Duration _wakeupCooldown = Duration(seconds: 30);

  static bool get hasRegisteredToken => _hasRegisteredToken;

  /// Called by main_shell.dart when app lifecycle changes.
  static void setForegroundState(bool isForeground) {
    _isInForeground = isForeground;
  }

  /// Returns true if a notification was suppressed by cooldown.
  static bool get isOnCooldown {
    if (_lastNotificationTime == null) return false;
    return DateTime.now().difference(_lastNotificationTime!) < _notificationCooldown;
  }

  /// Initializes the alert service and sets up listeners for native events.
  static void initialize() {
    _channel.setMethodCallHandler((call) async {
      switch (call.method) {
        case 'onDeviceToken':
          _apnsToken = call.arguments as String;
          final masked = _apnsToken!.length > 8 ? '...${_apnsToken!.substring(_apnsToken!.length - 8)}' : _apnsToken;
          debugPrint("🔔 AlertService: Received APNs Token: $masked");
          tryRegisterPendingToken();
          break;
        case 'onWakeup':
          // Cooldown: break the FCM echo loop (fetchMailbox → RBN sends push → onWakeup → fetchMailbox)
          if (_lastWakeupTime != null && DateTime.now().difference(_lastWakeupTime!) < _wakeupCooldown) {
            debugPrint("🔔 AlertService: Wakeup suppressed (30s cooldown — FCM echo loop breaker)");
            break;
          }
          _lastWakeupTime = DateTime.now();
          debugPrint("🔔 AlertService: Background Wakeup! Triggering P2P Fetch...");
          IntrovertClient().setAppIdleState(false);
          IntrovertClient().fetchMailbox();
          break;
        case 'onPushNotification':
          final args = call.arguments as Map<dynamic, dynamic>;
          final openChat = args['open_chat'] as String?;
          final openGroup = args['open_group'] as String?;
          final incomingCall = args['incoming_call'] as String?;
          final fcmToken = args['fcm_token'] as String?;
          
          if (fcmToken != null && fcmToken.isNotEmpty) {
            debugPrint("🔔 AlertService: FCM token received, caching for registration...");
            _pendingFcmToken = fcmToken;
            tryRegisterPendingToken();
          }
          
          debugPrint("🔔 AlertService: Push notification received: chat=$openChat, group=$openGroup, call=$incomingCall");
          IntrovertClient().fetchMailbox();
          break;
      }
    });
  }

  static String? get apnsToken => _apnsToken;

  /// Attempts to register the pending FCM or APNs token with the active RBN.
  static void tryRegisterPendingToken() {
    final token = _apnsToken ?? _pendingFcmToken;
    if (token == null || token.isEmpty) return;
    
    final client = IntrovertClient();
    try {
      client.registerPushToken(Platform.isIOS ? "ios" : "android", token);
      debugPrint("🔔 AlertService: Push token successfully registered with RBN.");
      _hasRegisteredToken = true;
      _pendingFcmToken = null;
      BackgroundSyncService.instance.updatePushAvailability(true);
    } catch (e) {
      debugPrint("🔔 AlertService: Engine not ready yet to register push token: $e");
    }
  }

  // ── Public API ─────────────────────────────────────────────────────────────

  /// Shows a local notification with the Introvert ping sound.
  ///
  /// **Cooldown:** Only one phone notification every 3 minutes. All others
  /// are silently suppressed.
  ///
  /// **Foreground:** When the app is open, no phone notification is posted.
  /// The caller should play an in-app sound instead.
  ///
  /// - [title]: bold heading in the notification banner.
  /// - [body]: message preview text.
  /// - [isCall]: when `true` the notification uses a CALL category/channel
  ///   (higher visual priority, different vibration pattern).
  static Future<void> showAlert({
    required String title,
    required String body,
    required bool isCall,
  }) async {
    // FOREGOUND: Skip native notification entirely. Caller plays sound.
    if (_isInForeground) {
      debugPrint("🔔 AlertService: Suppressed notification (app in foreground): $title");
      return;
    }

    // COOLDOWN: Skip if less than 3 minutes since last notification.
    if (isOnCooldown) {
      debugPrint("🔔 AlertService: Suppressed notification (3-min cooldown): $title");
      return;
    }

    // Ensure permissions have been requested at least once.
    if (!_permissionsRequested) {
      await requestPermissions();
    }

    try {
      await _channel.invokeMethod('showAlert', {
        'title': title,
        'body': body,
        'isCall': isCall,
      });
      _lastNotificationTime = DateTime.now();
      debugPrint("🔔 AlertService: Alert sent → title='$title' isCall=$isCall");
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to send native notification: $e");
    }
  }

  /// Explicitly requests notification permissions from the native layer.
  static Future<void> requestPermissions() async {
    _permissionsRequested = true;
    if (!Platform.isAndroid && !Platform.isIOS) return;

    try {
      await _channel.invokeMethod('requestPermissions');
    } on MissingPluginException {
      debugPrint("🔔 AlertService: requestPermissions not implemented natively (OK)");
    } catch (e) {
      debugPrint("🔔 AlertService: requestPermissions error: $e");
    }
  }

  /// Starts the native background foreground service (Android only).
  static Future<void> startBackgroundService({bool awake = false}) async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod('startBackgroundService', {'awake': awake});
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to start background service: $e");
    }
  }

  /// Stops the native background foreground service (Android only).
  static Future<void> stopBackgroundService() async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod('stopBackgroundService');
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to stop background service: $e");
    }
  }

  /// Sets whether the app should stay awake in the background (Android only).
  static Future<void> setStayAwake(bool awake) async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod('setStayAwake', {'awake': awake});
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to set stay awake: $e");
    }
  }
}
