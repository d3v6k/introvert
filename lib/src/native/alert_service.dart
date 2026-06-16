import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'introvert_client.dart';

/// Thin Dart wrapper around the native `introvert/alerts` MethodChannel.
///
/// Responsibilities:
/// - Request notification permissions on first use (Android 13+ / iOS).
/// - Route `showAlert` calls to the native side which posts a
///   `UNUserNotificationCenter` notification (iOS) or a
///   `NotificationCompat` notification (Android) and plays the Introvert ping.
class AlertService {
  static const MethodChannel _channel = MethodChannel('introvert/alerts');

  /// Set to `true` after the first permission request so we don't spam the
  /// native side on every incoming message.
  static bool _permissionsRequested = false;

  static String? _apnsToken;

  /// Initializes the alert service and sets up listeners for native events.
  static void initialize() {
    _channel.setMethodCallHandler((call) async {
      switch (call.method) {
        case 'onDeviceToken':
          _apnsToken = call.arguments as String;
          debugPrint("🔔 AlertService: Received APNs Token: $_apnsToken");
          IntrovertClient().registerPushToken(Platform.isIOS ? "ios" : "android", _apnsToken!);
          break;
        case 'onWakeup':
          debugPrint("🔔 AlertService: Background Wakeup! Triggering P2P Fetch...");
          IntrovertClient().fetchMailbox();
          break;
        case 'onPushNotification':
          // Push notification received from FCM/APNS
          final args = call.arguments as Map<dynamic, dynamic>;
          final openChat = args['open_chat'] as String?;
          final openGroup = args['open_group'] as String?;
          final incomingCall = args['incoming_call'] as String?;
          final fcmToken = args['fcm_token'] as String?;
          
          // Register FCM token with RBN if provided
          if (fcmToken != null && fcmToken.isNotEmpty) {
            debugPrint("🔔 AlertService: FCM token received, registering with RBN...");
            IntrovertClient().registerPushToken("android", fcmToken);
          }
          
          debugPrint("🔔 AlertService: Push notification received: chat=$openChat, group=$openGroup, call=$incomingCall");
          // Trigger mailbox fetch to get the actual message
          IntrovertClient().fetchMailbox();
          break;
      }
    });
  }

  static String? get apnsToken => _apnsToken;

  // ── Public API ─────────────────────────────────────────────────────────────

  /// Shows a local notification with the Introvert ping sound.
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
      debugPrint(
        "🔔 AlertService: Alert sent → title='$title' isCall=$isCall",
      );
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to send native notification: $e");
    }
  }

  /// Explicitly requests notification permissions from the native layer.
  ///
  /// On Android < 13 this is a no-op (permissions not required at runtime).
  /// On iOS the system shows a system dialog on first call.
  /// Safe to call multiple times — the OS handles de-duplication.
  static Future<void> requestPermissions() async {
    _permissionsRequested = true;
    if (!Platform.isAndroid && !Platform.isIOS) return;

    try {
      await _channel.invokeMethod('requestPermissions');
    } on MissingPluginException {
      // Older native side that doesn't handle this method — silently ignore.
      debugPrint("🔔 AlertService: requestPermissions not implemented natively (OK)");
    } catch (e) {
      debugPrint("🔔 AlertService: requestPermissions error: $e");
    }
  }

  /// Starts the native background foreground service (Android only).
  static Future<void> startBackgroundService() async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod('startBackgroundService');
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
  /// Typically enabled for Anchor Nodes to maintain high-performance mesh duties.
  static Future<void> setStayAwake(bool awake) async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod('setStayAwake', {'awake': awake});
    } catch (e) {
      debugPrint("🔔 AlertService: Failed to set stay awake: $e");
    }
  }
}
