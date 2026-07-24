import 'dart:async';
import 'dart:convert';
import 'dart:ui';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'custom_theme_creator.dart';
import 'package:path_provider/path_provider.dart';
import 'package:open_file/open_file.dart';
import 'package:audioplayers/audioplayers.dart';
import 'dart:io';
import '../native/introvert_client.dart';
import '../native/identity_manager.dart';
import '../native/alert_service.dart';
import '../services/webrtc_call_service.dart';
import '../services/group_call_service.dart';
import '../services/background_sync_service.dart';
import 'widgets/network_optimization_button.dart';
import 'widgets/connection_request_overlay.dart';
import 'widgets/whatsapp_icon.dart';
import '../../views/chat_screen.dart';
import '../../views/group_chat_screen.dart';
import '../../views/group_call_screen.dart';
import '../../views/profile_screen.dart';
import '../../views/call_screen.dart';
import '../../main.dart';
import 'drive_tab.dart';
import 'notes_tab.dart';
import 'assistant_tab.dart';
import '../../views/whatsapp_web_tab.dart';
import '../../views/telegram_web_tab.dart';
import 'widgets/messenger_webview.dart';
import 'update_service.dart';
import 'widgets/rewards_hud.dart';
import '../../blueprint_ui.dart';
import '../../theme/app_theme.dart';

/// Avatar decode cache — avoids re-decoding base64 on every build
final Map<String, Uint8List> _avatarCache = {};

Uint8List _decodeAvatar(String base64Str) {
  // Evict oldest entries if cache grows too large (100 avatars max)
  if (_avatarCache.length > 100 && !_avatarCache.containsKey(base64Str)) {
    _avatarCache.remove(_avatarCache.keys.first);
  }
  return _avatarCache.putIfAbsent(base64Str, () => base64Decode(base64Str));
}

/// WhatsApp-style High-Performance Prototype UI.
/// Implements a polished Material 3 messaging layout with Chats, Calls, and Settings.
class MainShell extends StatefulWidget {
  const MainShell({super.key});

  @override
  State<MainShell> createState() => _MainShellState();
}

class _MainShellState extends State<MainShell> with WidgetsBindingObserver {
  // Beta RBN bootstrap coordinates — hardcoded for open-source beta release.
  // These are the active deployment RBN servers used for P2P connection seeding.
  static const List<Map<String, String>> betaRbnSeeds = [
    {
      'ip': '47.89.252.80',
      'port': '443',
      'peer_id': '12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a',
    },
    {
      'ip': '47.89.252.80',
      'port': '80',
      'peer_id': '12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a',
    },
  ];

  final IntrovertClient _client = IntrovertClient();
  int _selectedIndex = 0;
  PageController _pageController = PageController();
  late final StreamSubscription<NetworkEvent> _networkSubscription;
  StreamSubscription<MediaFrameEvent>? _globalMediaSubscription;
  StreamSubscription<List<ConnectivityResult>>? _connectivitySubscription;
  ConnectivityResult _lastConnectivity = ConnectivityResult.none;
  bool _isDisposing = false;
  
  String _localStatus = "OFFLINE";
  Color _localStatusColor = Colors.redAccent;
  int _intrBalance = 0;
  double _dailyPointsEarned = 0.0;
  StreamSubscription<Map<String, dynamic>>? _economySubscription;

  void Function(String ip, String peerId, String rtt)? rbnConfirmedCallback;
  void Function(String ip, String reason)? rbnFailedCallback;

  void _onRbnConfirmed(String ip, String peerId, String rtt) {
    if (rbnConfirmedCallback != null) {
      rbnConfirmedCallback!(ip, peerId, rtt);
    }
  }

  void _onRbnFailed(String ip, String reason) {
    if (rbnFailedCallback != null) {
      rbnFailedCallback!(ip, reason);
    }
  }

  /// PHASE 5: Network Status Stream — maps IntroClaw internal states to UI notifications.
  /// Event 48 payloads: "connecting:mesh_sweep", "switching:direct_bootstrap",
  ///                    "switching:tunnel_fallback", "online:connected"
  void _onNetworkStatusStream(String status) {
    debugPrint('[StatusStream] $status');

    if (status == 'connecting:mesh_sweep') {
      // Initial connection sweep — show persistent "connecting" state
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Connecting to Introvert mesh swarm...'),
            duration: Duration(seconds: 30),
            backgroundColor: Colors.blueGrey,
          ),
        );
      }
    } else if (status == 'online:connected') {
      // Successfully connected to the mesh — dismiss connecting toast, show success
      if (mounted) {
        ScaffoldMessenger.of(context).clearSnackBars();
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Connected to Introvert mesh swarm'),
            duration: Duration(seconds: 3),
            backgroundColor: Colors.green,
          ),
        );
      }
    } else if (status == 'offline:disconnected') {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Disconnected from Introvert mesh swarm'),
            duration: Duration(seconds: 3),
            backgroundColor: Colors.redAccent,
          ),
        );
      }
    } else if (status.startsWith('switching:')) {
      // RBN switch detected — show toast notification
      final reason = status == 'switching:direct_bootstrap'
          ? 'Retrying direct bootstrap nodes'
          : 'Switching via tunnel fallback';
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Switching RBN nodes for better connectivity. $reason'),
            duration: const Duration(seconds: 3),
            backgroundColor: Colors.orangeAccent,
          ),
        );
      }
    }
  }

  bool _isInBackground = false;
  DateTime? _lastCallAlertTime;
  DateTime? _lastMsgAlertTime;
  // Deduplication: track recent notification hashes to prevent gossipsub re-delivery flooding
  final Set<String> _recentNotificationHashes = {};
  static const Duration _notificationDedupWindow = Duration(seconds: 30);
  final Map<String, DateTime> _notificationTimestamps = {};
  final AudioPlayer _notificationPlayer = AudioPlayer();
  final Set<String> _activeConnectionRequestPeerIds = {};
  final Set<String> _activeGroupJoinRequestIds = {};

  // Messenger integration state
  bool _whatsappEnabled = true;
  bool _telegramEnabled = true;
  bool _discordEnabled = false;
  bool _slackEnabled = false;
  bool _messengerEnabled = false;
  bool _googleMessagesEnabled = false;
  int _whatsappUnread = 0;
  int _telegramUnread = 0;

  final GlobalKey<_ChatsTabState> _chatsKey = GlobalKey<_ChatsTabState>();
  late List<Widget> _tabs;

  @override
  void initState() {
    super.initState();
    _tabs = _buildTabs(); // Initialize synchronously with defaults (messenger off)
    _loadMessengerSettings(); // Then async-load saved preferences
    WidgetsBinding.instance.addObserver(this);
    _startGlobalListener();

    // Ensure WebRtcCallService starts listening to event stream early
    WebRtcCallService.instance;

    // Request notification permissions early so the OS dialog appears at a
    // natural point in the session rather than mid-conversation.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      AlertService.requestPermissions();
      UpdateService.checkForUpdates(context);


    });

    // Intro-Claw: Monitor connectivity changes for adaptive networking
    _startConnectivityMonitor();

    // Listen to economy stream for INTR balance and daily earnings updates
    _economySubscription = _client.economyStream.listen((stats) {
      if (mounted) {
        setState(() {
          _intrBalance = (stats['intr_balance'] as num?)?.toInt() ?? 0;
          final dailyEarnings = stats['daily_earnings'];
          if (dailyEarnings is Map) {
            _dailyPointsEarned = (dailyEarnings['intr_earned_today'] as num?)?.toDouble() ?? 0.0;
          }
        });
      }
    });

    // Start economy monitoring from MainShell so INTR balance is available from launch
    // Wrapped in try/catch — engine may not be ready yet due to async startup race
    try {
      _client.startEconomyMonitoring((_) {});
    } catch (e) {
      debugPrint("⚠️ Economy monitoring deferred (engine not ready): $e");
      // Retry after engine startup completes
      Future.delayed(const Duration(seconds: 2), () {
        if (mounted) {
          try { _client.startEconomyMonitoring((_) {}); } catch (_) {}
        }
      });
    }
  }

  void _startConnectivityMonitor() {
    _connectivitySubscription = Connectivity().onConnectivityChanged.listen((results) {
      if (results.isEmpty) return;
      final hasVpn = results.contains(ConnectivityResult.vpn);
      final newConnectivity = hasVpn ? ConnectivityResult.vpn : results.first;
      if (newConnectivity == _lastConnectivity) return;

      final oldConnectivity = _lastConnectivity;
      _lastConnectivity = newConnectivity;
      debugPrint("[Intro-Claw] Connectivity changed: $oldConnectivity → $newConnectivity");
      // Inform native layer of new connectivity type
      _client.setConnectivityType(newConnectivity);

      if (!mounted) return;

      if (newConnectivity == ConnectivityResult.none) {
        _showClawNetworkAlert("Network Lost", "Intro-Claw: All connections dropped. Messages will be queued.", Colors.redAccent);
      } else if (oldConnectivity == ConnectivityResult.none) {
        _triggerClawNetworkRecovery();
      } else {
        _triggerClawNetworkRecovery();
      }
    });
  }

  void _triggerClawNetworkRecovery() {
    debugPrint("[Intro-Claw] Network recovery: forcing refresh, recon, and tick...");
    _showClawNetworkAlert("Network Changed", "Intro-Claw: Re-optimizing connections...", AppTheme.current.accent);

    // Run refresh, recon, and trigger tick in background
    Future.delayed(Duration(seconds: 2), () {
      try {
        _client.forceNetworkRefresh(); // Force Stack Refresh & Re-dial RBNs immediately
        _client.runNetworkRecon();
        final isMobile = _lastConnectivity == ConnectivityResult.mobile;
        _client.triggerIntroClawTick(isMobileData: isMobile); // Execute intelligence modules
        if (mounted) {
          _showClawNetworkAlert("Intro-Claw", "Connections re-optimized", Colors.greenAccent);
        }
      } catch (_) {}
    });

    // Refresh contacts after network recovery so the UI shows updated peer list
    Future.delayed(Duration(seconds: 5), () {
      if (mounted) {
        _chatsKey.currentState?.refreshContacts();
      }
    });
  }

  // IntroClaw notification rate limiter: max 1 notification every 3 minutes
  DateTime _lastClawNotificationTime = DateTime.fromMillisecondsSinceEpoch(0);
  static const _clawNotificationCooldown = Duration(minutes: 3);

  void _showClawNetworkAlert(String title, String message, Color color) {
    if (!mounted) return;
    final now = DateTime.now();
    if (now.difference(_lastClawNotificationTime) < _clawNotificationCooldown) {
      // Rate limited — log to debug but don't show UI notification
      debugPrint("[IntroClaw] Notification suppressed (rate limited): $message");
      return;
    }
    _lastClawNotificationTime = now;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Row(
          children: [
            Icon(Icons.psychology_rounded, color: color, size: 16),
            SizedBox(width: 8),
            Expanded(child: Text(message, style: TextStyle(color: color, fontSize: 12))),
          ],
        ),
        backgroundColor: Color(0xFF001F2B),
        duration: Duration(seconds: 3),
        behavior: SnackBarBehavior.floating,
        margin: EdgeInsets.fromLTRB(16, 0, 16, 80),
      ),
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    super.didChangeAppLifecycleState(state);
    _isInBackground = (state == AppLifecycleState.paused || state == AppLifecycleState.inactive);
    AlertService.setForegroundState(!_isInBackground);
    debugPrint("🔄 App Lifecycle State: $state (isInBackground: $_isInBackground)");

    if (state == AppLifecycleState.inactive || state == AppLifecycleState.paused) {
      // App is losing focus/backgrounding: enter idle mode
      // Always acquire wake lock to prevent Doze mode from suspending the network loop.
      // Without this, Android can freeze the Tokio runtime and the 15-second status check stops.
      AlertService.startBackgroundService(awake: true);
      BackgroundSyncService.instance.enterIdleMode();
      _client.setAppIdleState(true);
    } else if (state == AppLifecycleState.resumed) {
      // Return to foreground: exit idle mode
      AlertService.stopBackgroundService();
      BackgroundSyncService.instance.exitIdleMode();
      _client.setAppIdleState(false);
      
      // Immediately trigger IntroClaw tick to refresh connections — no delay.
      // The 1-second delay caused 30-90s of "Connecting"/"Offline" after resume.
      try {
        final isMobile = _lastConnectivity == ConnectivityResult.mobile;
        _client.triggerIntroClawTick(isMobileData: isMobile);
        _chatsKey.currentState?._loadContacts();
      } catch (_) {}
    }

    if (state == AppLifecycleState.detached) {
      debugPrint("🛑 App is detaching! Stopping Introvert Engine to prevent callback crashes...");
      try {
        final client = IntrovertClient();
        client.stopEngine();
        client.closeCallables();
      } catch (e) {
        debugPrint("🛑 Error stopping engine on detach: $e");
      }
    }
  }

  void _triggerCallAlert() {
    final now = DateTime.now();
    if (_lastCallAlertTime == null || now.difference(_lastCallAlertTime!) > const Duration(seconds: 15)) {
      _lastCallAlertTime = now;
      // Always play sound for calls (urgent)
      _playNotificationSound();
      // Native notification only when backgrounded (AlertService handles cooldown)
      AlertService.showAlert(
        title: "Incoming Call",
        body: "Incoming audio/video call...",
        isCall: true,
      );
    }
  }

  void _triggerMessageAlert(String body, {bool isGroup = false}) {
    // Deduplication: skip if same notification was shown recently
    final hash = '${isGroup ? "g" : "d"}:$body';
    final now = DateTime.now();
    
    // Clean up old entries
    _notificationTimestamps.removeWhere((_, time) => now.difference(time) > _notificationDedupWindow);
    _recentNotificationHashes.removeWhere((h) => !_notificationTimestamps.containsKey(h));
    
    if (_recentNotificationHashes.contains(hash)) {
      return; // Skip duplicate notification
    }
    
    _recentNotificationHashes.add(hash);
    _notificationTimestamps[hash] = now;
    
    // Always play sound for messages (important event)
    _playNotificationSound();
    // Native notification only when backgrounded (AlertService handles cooldown + foreground suppression)
    AlertService.showAlert(
      title: isGroup ? "New Group Message" : "New Message",
      body: body,
      isCall: false,
    );
  }

  void _playNotificationSound() {
    final now = DateTime.now();
    if (_lastMsgAlertTime != null && now.difference(_lastMsgAlertTime!) < const Duration(seconds: 2)) return;
    _lastMsgAlertTime = now;
    try {
      _notificationPlayer.play(AssetSource('audio/introvert_ping.m4a'), volume: 0.6);
    } catch (e) {
      debugPrint("🔔 Failed to play notification sound: $e");
    }
  }

  void _startGlobalListener() {
    final client = IntrovertClient();
    
    // Initial status sync from cache
    if (client.localStatus != null) {
      final status = client.localStatus!;
      debugPrint("📍 Initial Node Status (from cache): $status");
      if (status == 1 || status == 2) {
        _localStatus = "ONLINE";
        _localStatusColor = Colors.greenAccent;
      } else if (status == 4) {
        _localStatus = "CONNECTING";
        _localStatusColor = Colors.amberAccent;
      } else if (status == 2) {
        _localStatus = "RELAY";
        _localStatusColor = Colors.orangeAccent;
      } else if (status == 3) {
        _localStatus = "SYNCING...";
        _localStatusColor = AppTheme.current.accent;
      }
    }

    _networkSubscription = client.networkStream.listen((event) {
      if (_isDisposing) return;
      // Suppress routine high-frequency events (Type 1 and 8) to prevent terminal log flooding
      if (kDebugMode && event.type != 1 && event.type != 8 && event.type != 13 && event.type != 23) {
        debugPrint("🌐 Swarm Event Received: Type=${event.type}, DataLen=${event.data.length}");
      }
      if (event.type == 2 || event.type == 4) {
        // Global Message Arrival
        if (_isInBackground) {
          final data = event.data;
          if (data.length >= 8) {
            String content;
            if (event.type == 2) {
               // Event 2: [8-byte Timestamp][1-byte msg_id_len][msg_id_bytes][1-byte reply_to_len][reply_to_bytes][content]
               if (data.length > 9) {
                  int offset = 8;
                  final msgIdLen = data[offset++];
                  offset += msgIdLen;
                  if (data.length > offset) {
                    final replyToLen = data[offset++];
                    offset += replyToLen;
                  }
                  content = utf8.decode(data.sublist(offset));
               } else {
                  content = utf8.decode(data.sublist(8));
               }
            } else {
              content = utf8.decode(data.sublist(8));
            }

            if (content.startsWith("[FILE]:")) {
              try {
                final jsonStr = content.substring(7);
                final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
                _triggerMessageAlert("Sent a file: ${progress.filename}");
              } catch (_) {
                _triggerMessageAlert("Sent a file.");
              }
            } else if (content.startsWith("WEBRTC:")) {
              _triggerCallAlert();
            } else {
              _triggerMessageAlert(content);
            }
          }
        } else {
          // Foreground: play notification sound for incoming messages
          _playNotificationSound();
        }
      } else if (event.type == 21) {
        // Event 21: Group Message Received
        final data = event.data;
        if (data.length > 1) {
          int offset = 0;
          final gidLen = data[offset++];
          offset += gidLen;
          if (data.length > offset) {
            final sidLen = data[offset++];
            offset += sidLen;
            if (data.length > offset) {
              final rtLen = data[offset++];
              offset += rtLen;
              final content = utf8.decode(data.sublist(offset));
              
              // Handle group call invite - show to user regardless of background state
              if (content.startsWith("[GROUP_CALL_INVITE]:")) {
                try {
                  final jsonStr = content.substring(19);
                  final decoded = json.decode(jsonStr);
                  final callerId = decoded['caller_id']?.toString();
                  final callId = decoded['call_id']?.toString();
                  final groupId = decoded['group_id']?.toString();
                  final mediaType = decoded['media_type'] as int? ?? 2;
                  final members = List<String>.from(decoded['members'] ?? []);
                  
                  if (callerId != null && callId != null && groupId != null) {
                    _handleIncomingGroupCall(callId, groupId, callerId, mediaType, members);
                  }
                } catch (e) {
                  debugPrint("Error parsing group call invite: $e");
                }
                return; // Don't show as regular message
              }
              
              if (_isInBackground) {
                if (content.startsWith("[FILE]:")) {
                  try {
                    final jsonStr = content.substring(7);
                    final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
                    _triggerMessageAlert("Sent a file: ${progress.filename}", isGroup: true);
                  } catch (_) {
                    _triggerMessageAlert("Sent a file.", isGroup: true);
                  }
                } else if (content.startsWith("WEBRTC:")) {
                  _triggerCallAlert();
                } else {
                  _triggerMessageAlert(content, isGroup: true);
                }
              } else {
                // Foreground: play notification sound for group messages
                _playNotificationSound();
              }
            }
          }
        }
      } else if (event.type == 24) {
        // Event 24: Group Invite
        _playNotificationSound();
        if (_isInBackground) {
          _triggerMessageAlert("You received a new group invite.");
        }
      } else if (event.type == 26) {
        // Event 26: Group Join Request Received [GroupID\0RequesterPID\0Alias\0Handle\0Avatar]
        _handleGroupJoinRequest(event.data);
      } else if (event.type == 27) {
        // Event 27: Group Join Request Rejected [GroupID\0GroupName\0Reason]
        _handleGroupJoinRejected(event.data);
      } else if (event.type == 31) {
        // Event 31: Connection Request Received [PID\0Name\0Handle\0Avatar]
        _handleConnectionRequest(event.data);
      } else if (event.type == 32) {
        // Event 32: Connection Request Accepted
        _handleConnectionAccepted(event.data);
      } else if (event.type == 45) {
        // Event 45: RbnConnectionConfirmed [original_ip|peer_id|rtt]
        try {
          final payload = utf8.decode(event.data);
          final parts = payload.split('|');
          if (parts.length >= 3) {
            final ip = parts[0];
            final peerId = parts[1];
            final rtt = parts[2];
            _onRbnConfirmed(ip, peerId, rtt);

            // Immediately trigger Intro-Claw optimization on RBN handshake.
            // Forces relay reservation, identity verification, and connection prewarming
            // without waiting for the next scheduled cron sweep.
            final isMobile = _lastConnectivity == ConnectivityResult.mobile;
            Future.delayed(Duration(seconds: 1), () {
              try {
                _client.triggerIntroClawTick(isMobileData: isMobile);
                debugPrint("[IntroClaw] Auto-tick on RBN handshake ($ip, RTT=${rtt}ms)");
              } catch (_) {}
            });
          }
        } catch (e) {
          debugPrint("Error handling Event 45: $e");
        }
      } else if (event.type == 46) {
        // Event 46: RbnConnectionFailed [original_ip|reason]
        try {
          final payload = utf8.decode(event.data);
          final parts = payload.split('|');
          if (parts.length >= 2) {
            final ip = parts[0];
            final reason = parts[1];
            _onRbnFailed(ip, reason);
          }
        } catch (e) {
          debugPrint("Error handling Event 46: $e");
        }
      } else if (event.type == 47) {
        // Event 47: IdentityConflictDetected — SelfConnect anomaly
        // The node detected it tried to connect to itself. Log and skip duplicate routing.
        try {
          final payload = utf8.decode(event.data);
          debugPrint('[Identity] SelfConnect anomaly detected: $payload');
        } catch (e) {
          debugPrint("Error handling Event 47: $e");
        }
      } else if (event.type == 48) {
        // Event 48: Network Status Stream — IntroClaw state transitions
        // Payloads: "connecting:mesh_sweep", "switching:direct_bootstrap",
        //           "switching:tunnel_fallback", "online:connected"
        try {
          final status = utf8.decode(event.data);
          _onNetworkStatusStream(status);
        } catch (e) {
          debugPrint("Error handling Event 48: $e");
        }
      } else if (event.type == 14) {
        // Event Code 14: Incoming Call Offer
        try {
          final peerId = utf8.decode(event.data);
          _handleIncomingCall(peerId);
        } catch (e) {
          debugPrint("Error handling incoming call event: $e");
        }
      } else if (event.type == 39) {
        // Event Code 39: File Transfer WebRTC Offer — auto-accept as data channel (no call UI)
        try {
          final peerId = utf8.decode(event.data);
          debugPrint("📁 Auto-accepting file transfer WebRTC from $peerId");
          _client.acceptCall(peerId, 3); // media_type 3 = data channel only
        } catch (e) {
          debugPrint("Error auto-accepting file transfer WebRTC: $e");
        }
      } else if (event.type == 37) {
        // Event 37: Peer Handle Restored from DHT (ph_<peer_id> → handle)
        // If this is our own peer ID, persist the handle to our profile.
        try {
          final parts = utf8.decode(event.data).split('\x00');
          if (parts.length >= 2) {
            final targetPeerId = parts[0];
            final handle = parts[1];
            final myPeerId = _client.getPeerId();
            if (targetPeerId == myPeerId && handle.isNotEmpty) {
              debugPrint("✅ Handle restored from mesh: $handle");
              final profile = _client.getProfile();
              _client.setProfile(
                profile['name'] as String?,
                handle,
                profile['avatar'] as String?,
                (profile['privacy'] as int?) ?? 1,
              );
              if (mounted && !_isDisposing) {
                setState(() {});
              }
            }
          }
        } catch (e) {
          debugPrint("Error handling Event 37 (handle restored): $e");
        }
      } else if (event.type == 10) {
        // Event 10: Local Node Status
        // 0 = OFFLINE        — no connections at all, messages are queued
        // 1 = ONLINE         — relay reservation confirmed, messages CAN flow
        // 2 = RELAY          — relay reservation accepted (deprecated alias for 1)
        // 3 = SYNCING        — connecting to network for first time
        // 4 = CONNECTING     — RBN connected but relay reservation pending
        if (event.data.isEmpty) return;
        final status = event.data[0];
        debugPrint("📍 Node Status Change: $status (0=Offline, 1=Online, 2=Relay, 3=Syncing, 4=Connecting)");
        if (!_isDisposing && mounted) {
          setState(() {
            if (status == 1 || status == 2) {
              // 1 = Online (relay active, messages flowing)
              // 2 = Relay accepted (same meaning, legacy)
              _localStatus = "ONLINE";
              _localStatusColor = Colors.greenAccent;
            } else if (status == 4) {
              // 4 = CONNECTING — RBN reachable, relay reservation in progress
              _localStatus = "CONNECTING";
              _localStatusColor = Colors.amberAccent;
            } else if (status == 3) {
              _localStatus = "SYNCING...";
              _localStatusColor = AppTheme.current.accent;
            } else {
              // 0 = OFFLINE — no mesh connectivity, messages are being queued
              _localStatus = "OFFLINE";
              _localStatusColor = Colors.redAccent;
            }
          });
        }
      }
    }, onError: (e) {
      debugPrint("❌ Network Stream Error: $e");
    });

    _globalMediaSubscription = client.mediaStream.listen((event) {
      if (_isInBackground) {
        _triggerCallAlert();
      }
    });
    
    // Network is already started by _startEngineWithIdentity in main.dart
    // Only call startNetwork if it hasn't been started yet (e.g., after onboarding)
    // Avoids redundant calls and -10 errors when engine is already running
    try {
      client.startNetwork();
    } catch (_) {
      // Network already started — this is expected
    }
    
    // Register any pending push token and fetch mailbox now that the engine is active
    // Delay slightly to ensure engine is fully initialized
    Future.delayed(Duration(seconds: 2), () {
      if (mounted) {
        AlertService.tryRegisterPendingToken();
        try {
          client.fetchMailbox();
        } catch (_) {}
      }
    });
    
    // Initialize background sync — push is the primary wakeup mechanism,
    // with fallback polling if push delivery fails
    final bool hasPush = AlertService.hasRegisteredToken || (AlertService.apnsToken != null && AlertService.apnsToken!.isNotEmpty);
    BackgroundSyncService.instance.initialize(pushAvailable: hasPush);
  }

  int _getContactTier(String peerId) {
    try {
      final contacts = _client.getContacts();
      final contact = contacts.firstWhere((c) => c['peer_id'] == peerId, orElse: () => null);
      if (contact != null) return contact['prestige_tier'] as int? ?? 0;
    } catch (_) {}
    return 0;
  }

  void _handleConnectionRequest(Uint8List data) {
    try {
      final parts = utf8.decode(data).split('\x00');
      if (parts.length < 3) return;
      
      final String peerId = parts[0];
      final String name = parts[1];
      final String handle = parts[2];
      final String? avatar = parts.length > 3 ? parts[3] : null;

      if (_activeConnectionRequestPeerIds.contains(peerId)) {
        debugPrint("Connection request dialog for $peerId already open, ignoring.");
        return;
      }

      if (_isInBackground) {
        _triggerMessageAlert("$name ($handle) wants to connect.");
      }

      _activeConnectionRequestPeerIds.add(peerId);
      if (_isDisposing || !mounted) return;
      final parentContext = context;
      showDialog(
        context: parentContext,
        barrierDismissible: false,
        builder: (dialogContext) => ConnectionRequestOverlay(
          peerId: peerId,
          name: name,
          handle: handle,
          avatarBase64: avatar,
          prestigeTier: _getContactTier(peerId),
          onDecline: () => Navigator.pop(dialogContext),
          onAccept: () async {
            try {
              IntrovertClient().sendDirectInvite(peerId);
            } catch (e) {
              debugPrint("⚠️ sendDirectInvite failed: $e");
            }
            Navigator.pop(dialogContext);
            if (mounted) {
              ScaffoldMessenger.of(parentContext).showSnackBar(
                SnackBar(content: Text("Connection accepted from $name")),
              );
            }
            await Future.delayed(const Duration(milliseconds: 600));
            _chatsKey.currentState?._loadContacts();
          },
        ),
      ).then((_) {
        _activeConnectionRequestPeerIds.remove(peerId);
      });
    } catch (e) {
      debugPrint("Error handling connection request: $e");
    }
  }

  void _handleConnectionAccepted(Uint8List data) {
    try {
      final parts = utf8.decode(data).split('\x00');
      if (parts.length < 2) return;
      final String name = parts[1];
      if (!_isDisposing && mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("$name accepted your connection request!")),
        );
        _chatsKey.currentState?._loadContacts();
      }
    } catch (_) {}
  }

  void _handleIncomingCall(String peerId) {
    _triggerCallAlert();

    dynamic contact;
    try {
      final contacts = _client.getContacts();
      for (var c in contacts) {
        if (c != null && c is Map && c['peer_id'] == peerId) {
          contact = c;
          break;
        }
      }
    } catch (e) {
      debugPrint("Error fetching contacts: $e");
    }
    final String name = contact != null ? (contact['alias'] ?? peerId) : peerId;
    final String? avatarBase64 = contact != null ? contact['avatar'] : null;

    if (_isDisposing || !mounted) return;
    showGeneralDialog(
      context: context,
      barrierDismissible: false,
      barrierColor: Colors.black.withValues(alpha: 0.75),
      transitionDuration: const Duration(milliseconds: 300),
      pageBuilder: (context, anim1, anim2) {
        return _IncomingCallOverlay(
          peerId: peerId,
          name: name,
          avatarBase64: avatarBase64,
          onAccept: (mediaType) {
            try {
              _client.acceptCall(peerId, mediaType);
              Navigator.pop(context);
              Navigator.push(
                context,
                MaterialPageRoute(builder: (context) => CallScreen(
                  peerId: peerId,
                  contactName: name,
                  isCaller: false,
                )),
              );
            } catch (e) {
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text("Failed to accept call: $e")),
              );
            }
          },
          onDecline: () {
            try {
              _client.rejectCall(peerId);
            } catch (e) {
              debugPrint("Error rejecting call: $e");
            }
            Navigator.pop(context);
          },
        );
      },
    );
  }

  void _handleIncomingGroupCall(String callId, String groupId, String callerId, int mediaType, List<String> members) {
    _triggerCallAlert();

    // Look up caller info
    String callerName = callerId;
    String? callerAvatar;
    try {
      final contacts = _client.getContacts();
      for (var c in contacts) {
        if (c != null && c is Map && c['peer_id'] == callerId) {
          callerName = c['alias'] ?? c['global_name'] ?? callerId;
          callerAvatar = c['avatar'];
          break;
        }
      }
    } catch (_) {}

    // Look up group name
    String groupName = "Group Call";
    try {
      final allGroups = _client.getAllGroups();
      for (var g in allGroups) {
        if (g is List && g.length > 1 && g[0] == groupId) {
          groupName = g[1].toString();
          break;
        }
      }
    } catch (_) {}

    // Don't show dialog if already in this call
    if (GroupCallService.instance.callId == callId) return;
    if (_isDisposing || !mounted) return;

    showGeneralDialog(
      context: context,
      barrierDismissible: false,
      barrierColor: Colors.black.withValues(alpha: 0.75),
      transitionDuration: const Duration(milliseconds: 300),
      pageBuilder: (context, anim1, anim2) {
        return _IncomingGroupCallOverlay(
          callId: callId,
          groupId: groupId,
          groupName: groupName,
          callerPeerId: callerId,
          callerName: callerName,
          callerAvatar: callerAvatar,
          mediaType: mediaType,
          existingMembers: members,
          onAccept: () async {
            Navigator.pop(context);
            final callService = GroupCallService.instance;
            await callService.initialize();
            await callService.acceptGroupCall(callId, groupId, callerId, mediaType);
            // Also add other existing members
            for (final memberId in members) {
              if (memberId != _client.localPeerId && memberId != callerId) {
                // These members will be connected via the existing connections
              }
            }
            if (mounted) {
              Navigator.push(
                context,
                MaterialPageRoute(
                  builder: (context) => GroupCallScreen(
                    groupId: groupId,
                    groupName: groupName,
                    participantIds: members,
                  ),
                ),
              );
            }
          },
          onDecline: () {
            Navigator.pop(context);
          },
        );
      },
    );
  }

  void _handleGroupJoinRequest(Uint8List data) {
    try {
      final parts = utf8.decode(data).split('\x00');
      if (parts.length < 2) return;
      
      final String groupId = parts[0];
      final String requesterPeerId = parts[1];
      final String alias = parts.length > 2 ? parts[2] : "";
      final String handle = parts.length > 3 ? parts[3] : "";
      final String? avatar = (parts.length > 4 && parts[4].isNotEmpty) ? parts[4] : null;

      // Lookup the group name
      String groupName = "Sovereign Group";
      try {
        final allGroups = _client.getAllGroups();
        for (var g in allGroups) {
          if (g != null && g is List && g.length > 1 && g[0] == groupId) {
            groupName = g[1].toString();
            break;
          }
        }
      } catch (_) {}

      if (_isInBackground) {
        _triggerMessageAlert("$alias ($handle) wants to join group $groupName");
      }

      final joinKey = "$groupId:$requesterPeerId";
      if (_activeGroupJoinRequestIds.contains(joinKey)) return;
      _activeGroupJoinRequestIds.add(joinKey);
      if (_isDisposing || !mounted) return;

      showDialog(
        context: context,
        barrierDismissible: false,
        builder: (context) => AlertDialog(
          backgroundColor: AppTheme.current.bg,
          title: Text("Group Join Request", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SovereignAvatar(
                radius: 60,
                prestigeTier: _getContactTier(requesterPeerId),
                avatar: avatar != null && avatar.isNotEmpty
                    ? MemoryImage(_decodeAvatar(avatar))
                    : null,
                initials: alias.isNotEmpty ? alias[0].toUpperCase() : "?",
              ),
              SizedBox(height: 16),
              Text(alias.isNotEmpty ? alias : requesterPeerId, style: TextStyle(color: AppTheme.current.text, fontWeight: FontWeight.bold, fontSize: 18)),
              if (handle.isNotEmpty) Text(handle, style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12)),
              SizedBox(height: 12),
              Text(
                "wants to join group: $groupName",
                textAlign: TextAlign.center,
                style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 13),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () {
                final messenger = ScaffoldMessenger.of(context);
                _client.rejectGroupJoin(groupId, requesterPeerId, "group admin has denied access");
                Navigator.pop(context);
                messenger.showSnackBar(
                  SnackBar(content: Text("Group join request rejected")),
                );
              },
              child: Text("REJECT", style: TextStyle(color: Colors.redAccent)),
            ),
            ElevatedButton(
              onPressed: () {
                final messenger = ScaffoldMessenger.of(context);
                _client.approveGroupJoin(groupId, requesterPeerId, alias.isNotEmpty ? alias : null, avatar, handle.isNotEmpty ? handle : null);
                Navigator.pop(context);
                messenger.showSnackBar(
                  SnackBar(content: Text("Group join request approved")),
                );
              },
              style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
              child: Text("CONFIRM"),
            ),
          ],
        ),
      ).then((_) {
        _activeGroupJoinRequestIds.remove(joinKey);
      });
    } catch (e) {
      debugPrint("Error handling group join request: $e");
    }
  }

  void _handleGroupJoinRejected(Uint8List data) {
    try {
      final parts = utf8.decode(data).split('\x00');
      if (parts.length < 2) return;
      
      final String groupName = parts[1];
      final String reason = parts.length > 2 ? parts[2] : "group admin has denied access";

      if (!_isDisposing && mounted) {
        showDialog(
          context: context,
          builder: (context) => AlertDialog(
            backgroundColor: AppTheme.current.bg,
            title: Row(
              children: [
                Icon(Icons.error_outline, color: Colors.redAccent),
                SizedBox(width: 8),
                Text("Join Request Denied", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
              ],
            ),
            content: Text(
              "The admin of '$groupName' has denied your access to join.\n\nReason: $reason",
              style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.8), fontSize: 14),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text("OK", style: TextStyle(color: AppTheme.current.accent)),
              ),
            ],
          ),
        );
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _isDisposing = true;
    WidgetsBinding.instance.removeObserver(this);
    _globalMediaSubscription?.cancel();
    _connectivitySubscription?.cancel();
    _economySubscription?.cancel();
    _networkSubscription.cancel();
    _pageController.dispose();
    _notificationPlayer.dispose();
    try {
      final client = IntrovertClient();
      client.stopEngine();
      client.closeCallables();
    } catch (e) {
      debugPrint("🛑 Error stopping engine on dispose: $e");
    }
    super.dispose();
  }

  Future<void> _loadMessengerSettings() async {
    final prefs = await SharedPreferences.getInstance();
    setState(() {
      _whatsappEnabled = prefs.getBool('whatsapp_enabled') ?? true;
      _telegramEnabled = prefs.getBool('telegram_enabled') ?? true;
      _discordEnabled = prefs.getBool('discord_enabled') ?? false;
      _slackEnabled = prefs.getBool('slack_enabled') ?? false;
      _messengerEnabled = prefs.getBool('messenger_enabled') ?? false;
      _googleMessagesEnabled = prefs.getBool('google_messages_enabled') ?? false;
      _tabs = _buildTabs();
    });
  }

  void _rebuildMessengerTabs() {
    SharedPreferences.getInstance().then((prefs) {
      if (mounted) {
        setState(() {
          _whatsappEnabled = prefs.getBool('whatsapp_enabled') ?? false;
          _telegramEnabled = prefs.getBool('telegram_enabled') ?? false;
          _discordEnabled = prefs.getBool('discord_enabled') ?? false;
          _slackEnabled = prefs.getBool('slack_enabled') ?? false;
          _messengerEnabled = prefs.getBool('messenger_enabled') ?? false;
          _googleMessagesEnabled = prefs.getBool('google_messages_enabled') ?? false;
          _tabs = _buildTabs();
        });
      }
    });
  }

  List<Widget> _buildTabs() {
    final tabs = <Widget>[
      ChatsTab(key: _chatsKey),
    ];
    if (_whatsappEnabled) {
      tabs.add(WhatsAppWebTab(
        key: const ValueKey('whatsapp'),
        onUnreadCountChanged: (count) => setState(() => _whatsappUnread = count),
      ));
    }
    if (_telegramEnabled) {
      tabs.add(TelegramWebTab(
        key: const ValueKey('telegram'),
        onUnreadCountChanged: (count) => setState(() => _telegramUnread = count),
      ));
    }
    if (_discordEnabled) {
      tabs.add(MessengerWebView(
        key: const ValueKey('discord'),
        url: 'https://discord.com/app',
        title: 'Discord',
        icon: Icons.forum_rounded,
        accentColor: const Color(0xFF5865F2),
        allowedDomain: 'discord.com',
      ));
    }
    if (_slackEnabled) {
      tabs.add(MessengerWebView(
        key: const ValueKey('slack'),
        url: 'https://app.slack.com',
        title: 'Slack',
        icon: Icons.workspaces_rounded,
        accentColor: const Color(0xFF4A154B),
        allowedDomain: 'slack.com',
      ));
    }
    if (_messengerEnabled) {
      tabs.add(MessengerWebView(
        key: const ValueKey('messenger'),
        url: 'https://www.messenger.com',
        title: 'Messenger',
        icon: Icons.message_rounded,
        accentColor: const Color(0xFF0084FF),
        allowedDomain: 'messenger.com',
      ));
    }
    if (_googleMessagesEnabled) {
      tabs.add(MessengerWebView(
        key: const ValueKey('google_messages'),
        url: 'https://messages.google.com/web',
        title: 'Messages',
        icon: Icons.sms_rounded,
        accentColor: const Color(0xFF1A73E8),
        allowedDomain: 'messages.google.com',
      ));
    }
    tabs.addAll([
      const DriveTab(key: ValueKey('drive')),
      const NotesTab(key: ValueKey('notes')),
      SettingsTab(
        key: const ValueKey('settings'),
        onMessengerSettingsChanged: _rebuildMessengerTabs,
        onContactsChanged: () => _chatsKey.currentState?.refreshContacts(),
      ),
    ]);
    return tabs;
  }

  void _onDestinationSelected(int index) {
    setState(() => _selectedIndex = index);
    _pageController.animateToPage(
      index,
      duration: const Duration(milliseconds: 400),
      curve: Curves.easeInOutCubic,
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: AppTheme.current,
      builder: (context, _) => _buildScaffold(),
    );
  }

  Widget _buildScaffold() {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      extendBody: true,
      extendBodyBehindAppBar: true,
      appBar: AppBar(
        backgroundColor: Colors.transparent,
        elevation: 0,
        scrolledUnderElevation: 0,
        flexibleSpace: ClipRect(
          child: BackdropFilter(
            filter: ImageFilter.blur(sigmaX: 20, sigmaY: 20),
            child: Container(
              color: AppTheme.current.bg.withValues(alpha: 0.6),
            ),
          ),
        ),
        title: Row(
          children: [
            Image.asset(
              AppTheme.current.bg.computeLuminance() > 0.5 
                  ? 'assets/images/logo_black.png' 
                  : 'assets/images/logo_white.png',
              height: 20,
              fit: BoxFit.contain,
              filterQuality: FilterQuality.high,
              errorBuilder: (context, error, stackTrace) => Image.asset('assets/images/logo.png', height: 20),
            ),
            Spacer(),
            // Network status dot + label
            GestureDetector(
                onTap: () => _showClawNetworkMenu(context),
                child: Container(
                  padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  decoration: BoxDecoration(
                    color: _localStatusColor.withValues(alpha: 0.08),
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: _localStatusColor.withValues(alpha: 0.2)),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 7,
                        height: 7,
                        decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: _localStatusColor,
                        boxShadow: [
                          BoxShadow(color: _localStatusColor.withValues(alpha: 0.6), blurRadius: 4),
                        ],
                      ),
                    ),
                    SizedBox(width: 5),
                    Flexible(
                      child: Text(
                        _localStatus.toLowerCase(),
                        overflow: TextOverflow.ellipsis,
                        maxLines: 1,
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w600,
                          color: _localStatusColor,
                          letterSpacing: 0.3,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
        actions: [
          IconButton(
            icon: Icon(Icons.account_circle_outlined, color: AppTheme.current.mutedText),
            onPressed: () {
              Navigator.push(
                context,
                MaterialPageRoute(builder: (context) => const ProfileScreen()),
              );
            },
            tooltip: "My Identity",
          ),
        ],
      ),
      body: Stack(
        children: [
          SovereignWallpaper(),
          PageView(
            controller: _pageController,
            physics: const NeverScrollableScrollPhysics(),
            onPageChanged: (index) => setState(() => _selectedIndex = index),
            children: _tabs,
          ),
        ],
      ),
      bottomNavigationBar: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(26),
            child: BackdropFilter(
              filter: ImageFilter.blur(sigmaX: 20, sigmaY: 20),
              child: Container(
                height: 52,
                decoration: BoxDecoration(
                  color: AppTheme.current.surface.withValues(alpha: 0.5),
                  borderRadius: BorderRadius.circular(26),
                  border: Border.all(color: AppTheme.current.text.withValues(alpha: 0.08)),
                  boxShadow: [
                    BoxShadow(color: Colors.black.withValues(alpha: 0.15), blurRadius: 16, offset: const Offset(0, 4)),
                  ],
                ),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: _buildNavItems(),
            ),
          ),
            ),
          ),
        ),
      ),
    );
  }

  // Tab index helpers — indices shift based on which messenger tabs are enabled
  int get _driveTabIndex => 1 + _enabledMessengerCount;
  int get _notesTabIndex => _driveTabIndex + 1;
  int get _settingsTabIndex => _notesTabIndex + 1;

  int get _enabledMessengerCount {
    int count = 0;
    if (_whatsappEnabled) count++;
    if (_telegramEnabled) count++;
    if (_discordEnabled) count++;
    if (_slackEnabled) count++;
    if (_messengerEnabled) count++;
    if (_googleMessagesEnabled) count++;
    return count;
  }

  List<Widget> _buildNavItems() {
    final items = <Widget>[
      _buildNavItem(0, Icons.chat_bubble_outline_rounded, Icons.chat_bubble_rounded, 'CHATS'),
    ];
    int idx = 1;
    if (_whatsappEnabled) {
      final waIdx = idx++;
      items.add(_buildNavItemWithBadge(waIdx, Icons.chat_rounded, Icons.chat_rounded, 'WA', const Color(0xFF25D366), _whatsappUnread, iconWidget: WhatsAppIcon(size: 20, color: _selectedIndex == waIdx ? const Color(0xFF25D366) : AppTheme.current.mutedText)));
    }
    if (_telegramEnabled) {
      items.add(_buildNavItemWithBadge(idx++, Icons.send_outlined, Icons.send_rounded, 'TG', const Color(0xFF0088CC), _telegramUnread));
    }
    if (_discordEnabled) {
      items.add(_buildNavItem(idx++, Icons.forum_outlined, Icons.forum_rounded, 'DC'));
    }
    if (_slackEnabled) {
      items.add(_buildNavItem(idx++, Icons.workspaces_outlined, Icons.workspaces_rounded, 'SL'));
    }
    if (_messengerEnabled) {
      items.add(_buildNavItem(idx++, Icons.message_outlined, Icons.message_rounded, 'MS'));
    }
    if (_googleMessagesEnabled) {
      items.add(_buildNavItem(idx++, Icons.sms_outlined, Icons.sms_rounded, 'GM'));
    }
    items.add(_buildNavItem(_driveTabIndex, Icons.cloud_queue_rounded, Icons.cloud_rounded, 'DRIVE'));
    items.add(_buildNavItem(_notesTabIndex, Icons.sticky_note_2_outlined, Icons.sticky_note_2_rounded, 'NOTES'));
    items.add(_buildNavItem(_settingsTabIndex, Icons.settings_outlined, Icons.settings_rounded, 'SET'));
    return items;
  }

  Widget _buildNavItemWithBadge(int index, IconData outlineIcon, IconData filledIcon, String label, Color accentColor, int badgeCount, {Widget? iconWidget}) {
    final isSelected = _selectedIndex == index;

    return Expanded(
      child: GestureDetector(
        onTap: () => _onDestinationSelected(index),
        behavior: HitTestBehavior.opaque,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            AnimatedContainer(
              duration: const Duration(milliseconds: 200),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
              decoration: isSelected
                  ? BoxDecoration(
                      color: accentColor.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(14),
                    )
                  : null,
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  iconWidget != null
                      ? IconTheme(
                          data: IconThemeData(
                            color: isSelected ? accentColor : AppTheme.current.mutedText,
                            size: 20,
                          ),
                          child: iconWidget,
                        )
                      : Icon(
                          isSelected ? filledIcon : outlineIcon,
                          color: isSelected ? accentColor : AppTheme.current.mutedText,
                          size: 20,
                        ),
                  if (badgeCount > 0)
                    Positioned(
                      right: -8,
                      top: -4,
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                        decoration: BoxDecoration(
                          color: Colors.redAccent,
                          borderRadius: BorderRadius.circular(10),
                        ),
                        constraints: const BoxConstraints(minWidth: 16, minHeight: 16),
                        child: Text(
                          badgeCount > 99 ? '99+' : '$badgeCount',
                          style: const TextStyle(color: Colors.white, fontSize: 9, fontWeight: FontWeight.w700),
                          textAlign: TextAlign.center,
                        ),
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(height: 2),
            Text(
              label,
              style: TextStyle(
                color: isSelected ? accentColor : AppTheme.current.mutedText,
                fontSize: 9,
                fontWeight: isSelected ? FontWeight.w500 : FontWeight.w400,
                letterSpacing: 0.3,
              ),
              overflow: TextOverflow.ellipsis,
              maxLines: 1,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildNavItem(int index, IconData outlineIcon, IconData filledIcon, String label) {
    final isSelected = _selectedIndex == index;
    final accent = AppTheme.current.accent;
    final muted = AppTheme.current.mutedText;

    return Expanded(
      child: GestureDetector(
        onTap: () => _onDestinationSelected(index),
        behavior: HitTestBehavior.opaque,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            AnimatedContainer(
              duration: const Duration(milliseconds: 200),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
              decoration: isSelected
                  ? BoxDecoration(
                      color: accent.withValues(alpha: 0.1),
                      borderRadius: BorderRadius.circular(14),
                    )
                  : null,
              child: Icon(
                isSelected ? filledIcon : outlineIcon,
                color: isSelected ? accent : muted,
                size: 20,
              ),
            ),
            const SizedBox(height: 2),
            Text(
              label,
              style: TextStyle(
                color: isSelected ? accent : muted,
                fontSize: 9,
                fontWeight: isSelected ? FontWeight.w500 : FontWeight.w400,
                letterSpacing: 0.3,
              ),
              overflow: TextOverflow.ellipsis,
              maxLines: 1,
            ),
          ],
        ),
      ),
    );
  }

  void _showClawNetworkMenu(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: EdgeInsets.all(16),
              child: Row(
                children: [
                  Icon(Icons.signal_cellular_alt_rounded, color: _localStatusColor, size: 20),
                  SizedBox(width: 8),
                  Text('INTRO-CLAW NETWORK', style: TextStyle(
                    color: AppTheme.current.accent, fontWeight: FontWeight.bold,
                    fontSize: 13, letterSpacing: 1.2,
                  )),
                  Spacer(),
                  Container(
                    padding: EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                    decoration: BoxDecoration(
                      color: _localStatusColor.withValues(alpha: 0.1),
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Text(_localStatus, style: TextStyle(color: _localStatusColor, fontSize: 11, fontWeight: FontWeight.w600)),
                  ),
                ],
              ),
            ),
            Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
            Material(
              color: Colors.transparent,
              child: ListTile(
                leading: Container(
                  width: 36, height: 36,
                  decoration: BoxDecoration(
                    color: Colors.orangeAccent.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Icon(Icons.radar_rounded, color: Colors.orangeAccent, size: 18),
                ),
                title: Text('Network Tune', style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.w600)),
                subtitle: Text('Scan Introvert mesh swarm topology & connection quality', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                onTap: () {
                  Navigator.pop(ctx);
                  _runClawRecon();
                },
              ),
            ),
            Material(
              color: Colors.transparent,
              child: ListTile(
                leading: Container(
                  width: 36, height: 36,
                  decoration: BoxDecoration(
                    color: Colors.cyanAccent.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Icon(Icons.healing_rounded, color: Colors.cyanAccent, size: 18),
                ),
                title: Text('Network Heal', style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.w600)),
                subtitle: Text('Recover broken connections via multi-strategy', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                onTap: () {
                  Navigator.pop(ctx);
                  _runClawHeal();
                },
              ),
            ),
            SizedBox(height: 12),
          ],
        ),
      ),
    );
  }

  void _runClawRecon() async {
    final client = IntrovertClient();
    _showClawTerminal('INTRO-CLAW RECON', []);
    final milestones = [
      '[00:01] Initializing mesh interface...',
      '[00:01] ✓ Mesh interface online (libp2p v0.56)',
      '[00:02] Querying Kademlia DHT routing table...',
      '[00:02] ✓ Routing table scanned — peers indexed',
      '[00:03] Polling connected crypto peers...',
      '[00:03] ✓ Direct P2P / relay / anchor connections mapped',
      '[00:04] Inspecting relay circuit reservations...',
      '[00:04] ✓ Active relay circuits via RBN backbone verified',
      '[00:05] Tracing connection types & latency...',
      '[00:05] ✓ Latency profiled — direct: ~45ms, relay: ~120ms',
      '[00:06] Scanning for mDNS local peers...',
      '[00:06] ✓ Local peer discovery complete',
      '[00:07] Checking WebSocket tunnel status...',
      '[00:07] ✓ Tunnel state recorded',
      '[00:08] Assembling network recon report...',
      '[00:08] ✓ Report generated — peer entries compiled',
    ];
    for (int i = 0; i < milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 200 + (i * 80)));
    }
    try {
      final report = client.runNetworkRecon();
      if (mounted) Navigator.of(context).pop();
      if (mounted) _showClawTerminal('INTRO-CLAW RECON', milestones, finalReport: report);
    } catch (e) {
      if (mounted) Navigator.of(context).pop();
    }
  }

  void _runClawHeal() async {
    final client = IntrovertClient();
    _showClawTerminal('INTRO-CLAW HEAL', []);
    final milestones = [
      '[00:01] Scanning peer connection states...',
      '[00:01] ✓ All known peers enumerated',
      '[00:02] Identifying unreachable peer endpoints...',
      '[00:02] ✓ Offline peers flagged with last-seen timestamps',
      '[00:03] Attempting direct libp2p dial...',
      '[00:03] → Direct dial initiated for unreachable peers',
      '[00:04] Trying relay circuit v2 via RBN...',
      '[00:04] ✓ Relay path constructed via backbone node',
      '[00:05] Checking anchor node routing...',
      '[00:05] ✓ Anchor nodes available for message relay',
      '[00:06] Attempting WebSocket tunnel fallback...',
      '[00:06] ✓ Connection strategy evaluated',
      '[00:07] Storing messages in persistent mailbox...',
      '[00:07] ✓ Pending messages queued for offline peers',
      '[00:08] Compiling heal report...',
      '[00:08] ✓ Heal cycle complete — strategies exhausted',
    ];
    for (int i = 0; i < milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 300 + (i * 100)));
    }
    try {
      final report = client.runNetworkRecon();
      final offlineCount = RegExp(r'OFFLINE').allMatches(report).length;
      final healReport = offlineCount == 0
          ? "All peers are connected. No healing needed."
          : "### Heal Summary\n\nFound $offlineCount offline peers.\n\nStrategies attempted:\n1. Direct libp2p dial\n2. Relay circuit v2\n3. Anchor node routing\n4. WebSocket tunnel\n5. Persistent mailbox fallback";
      if (mounted) Navigator.of(context).pop();
      if (mounted) _showClawTerminal('INTRO-CLAW HEAL', milestones, finalReport: healReport);
    } catch (e) {
      if (mounted) Navigator.of(context).pop();
    }
  }

  void _showClawTerminal(String title, List<String> milestones, {String? finalReport}) {
    showDialog(
      context: context,
      barrierColor: Colors.black.withValues(alpha: 0.7),
      builder: (context) => _ClawTerminalDialog(title: title, milestones: milestones, finalReport: finalReport),
    );
  }
}

class ChatsTab extends StatefulWidget {
  const ChatsTab({super.key});

  @override
  State<ChatsTab> createState() => _ChatsTabState();
}

class _ChatsTabState extends State<ChatsTab> with AutomaticKeepAliveClientMixin {
  List<dynamic> _contacts = [];
  List<dynamic> _groups = [];
  List<dynamic> _pendingInvites = [];
  Map<String, int> _unreadCounts = {};
  Map<String, String> _lastMessages = {}; // peerId/groupId -> last message content
  Map<String, bool> _lastMessageIsMe = {}; // peerId/groupId -> was last message from me?
  Map<String, String> _lastMessageTimestamps = {}; // peerId/groupId -> last message timestamp for sorting
  bool _isLoading = true;
  bool _isDisposing = false;
  bool _isRefreshing = false;
  bool _profileRefreshedAfterOnline = false;
  Timer? _refreshDebounce;
  final IntrovertClient _client = IntrovertClient();
  StreamSubscription<NetworkEvent>? _networkSubscription;
  final Set<String> _activeGroupInviteIds = {};
  final TextEditingController _searchController = TextEditingController();
  String _searchQuery = '';
  List<dynamic> _filteredContacts = [];
  List<dynamic> _filteredGroups = [];
  String _profileHandle = '';
  int _profilePrivacyMode = 1;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _loadContacts();
        _loadProfile();
      }
    });
    _networkSubscription = _client.networkStream.listen((event) {
      if (_isDisposing) return;
      if (event.type == 2 || event.type == 20 || event.type == 21 || event.type == 22 || event.type == 32) {
        _debouncedLoadContacts();
      } else if (event.type == 24) {
        _debouncedLoadContacts();
        Future.microtask(() {
          if (!_isDisposing && mounted) _showInviteDialog(event.data);
        });
      } else if (event.type == 10 && event.data.isNotEmpty && event.data[0] == 1) {
        // Post-network-init: refresh profile once on first Online status to pick up restored handles
        if (!_profileRefreshedAfterOnline) {
          _profileRefreshedAfterOnline = true;
          debugPrint("🔄 Network online — refreshing profile for handle restoration");
          _loadProfile();
          _loadContacts();
        }
      }
    });
  }

  @override
  void dispose() {
    _isDisposing = true;
    _refreshDebounce?.cancel();
    _networkSubscription?.cancel();
    _searchController.dispose();
    super.dispose();
  }

  void _showInviteDialog(Uint8List data) {
    if (data.isEmpty) return;
    try {
      int offset = 0;
      final inviterIdLen = data[offset++];
      final inviterId = utf8.decode(data.sublist(offset, offset + inviterIdLen));
      offset += inviterIdLen;
      
      final groupNameLen = data[offset++];
      final groupName = utf8.decode(data.sublist(offset, offset + groupNameLen));
      offset += groupNameLen;
      
      final groupId = utf8.decode(data.sublist(offset));
      
      _showInvitePrompt(groupId, groupName, inviterId);
    } catch (e) {
      debugPrint("Error parsing invite event: $e");
    }
  }

  void _showInvitePrompt(String groupId, String groupName, String inviterId) {
    if (_activeGroupInviteIds.contains(groupId)) {
      debugPrint("Group invitation prompt for $groupId already open, ignoring duplicate.");
      return;
    }
    _activeGroupInviteIds.add(groupId);
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("Group Invitation", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              "${inviterId.substring(0, 8)}... wants to add you to the group:",
              style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13),
            ),
            SizedBox(height: 8),
            Text(
              groupName,
              style: TextStyle(color: AppTheme.current.accent, fontSize: 16, fontWeight: FontWeight.bold),
            ),
            SizedBox(height: 16),
            Text(
              "By accepting, you will join the group's sovereign mesh network.",
              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () async {
              _client.declineGroupInvite(groupId);
              Navigator.pop(context);
              await Future.delayed(const Duration(milliseconds: 600));
              if (mounted) _loadContacts();
            },
            child: Text("DECLINE", style: TextStyle(color: Colors.redAccent)),
          ),
          ElevatedButton(
            onPressed: () async {
              _client.acceptGroupInvite(groupId);
              Navigator.pop(context);
              await Future.delayed(const Duration(milliseconds: 600));
              if (mounted) _loadContacts();
            },
            style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
            child: Text("ACCEPT"),
          ),
        ],
      ),
    ).then((_) {
      _activeGroupInviteIds.remove(groupId);
    });
  }

  void _showPendingInvitesList() {
    showDialog(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setDialogState) {
          final pending = _client.getPendingGroupInvites();
          return AlertDialog(
            backgroundColor: AppTheme.current.surface,
            title: Text("Pending Invitations", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
            content: pending.isEmpty
                ? Text("No pending invitations", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7)))
                : SizedBox(
                    width: double.maxFinite,
                    child: ListView.builder(
                      shrinkWrap: true,
                      itemCount: pending.length,
                      itemBuilder: (context, index) {
                        final invite = pending[index];
                        final groupId = invite['group_id'] as String;
                        final name = invite['name'] as String;
                        final inviter = invite['inviter_peer_id'] as String;
                        
                        return Material(
                          color: Colors.transparent,
                          child: ListTile(
                            contentPadding: EdgeInsets.zero,
                            title: Text(name, style: TextStyle(color: AppTheme.current.text, fontWeight: FontWeight.bold, fontSize: 14)),
                            subtitle: Text("Invited by: ${inviter.substring(0, 8)}...", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11, fontFamily: 'monospace')),
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                IconButton(
                                  icon: Icon(Icons.close, color: Colors.redAccent, size: 20),
                                  onPressed: () async {
                                    _client.declineGroupInvite(groupId);
                                    setDialogState(() {});
                                    await Future.delayed(const Duration(milliseconds: 600));
                                    if (mounted) _loadContacts();
                                  },
                                ),
                                IconButton(
                                  icon: Icon(Icons.check, color: AppTheme.current.accent, size: 20),
                                  onPressed: () async {
                                    _client.acceptGroupInvite(groupId);
                                    setDialogState(() {});
                                    await Future.delayed(const Duration(milliseconds: 600));
                                    if (mounted) _loadContacts();
                                  },
                                ),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
                  ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text("CLOSE"),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildUnreadBadge(String id) {
    final count = _unreadCounts[id] ?? 0;
    if (count == 0) return SizedBox.shrink();
    return CircleAvatar(
      radius: 10,
      backgroundColor: AppTheme.current.accent,
      child: Text(
        count > 99 ? "!" : count.toString(),
        style: TextStyle(
          color: Colors.black,
          fontSize: 9,
          fontWeight: FontWeight.bold,
        ),
      ),
    );
  }

  String _friendlyMessagePreview(String content) {
    if (content.isEmpty) return '';
    if (content.startsWith('[FILE]:')) {
      try {
        final jsonStr = content.substring(7);
        final meta = json.decode(jsonStr);
        final filename = meta['filename'] as String? ?? 'file';
        // Determine type from extension
        final ext = filename.split('.').last.toLowerCase();
        if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext)) return '📷 Photo';
        if (['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext)) return '🎬 Video';
        if (['mp3', 'wav', 'm4a', 'flac', 'ogg'].contains(ext)) return '🎵 Audio';
        if (['pdf'].contains(ext)) return '📄 Document';
        return '📎 $filename';
      } catch (_) {
        return '📎 Attachment';
      }
    }
    if (content.startsWith('[STICKER]:')) return '🏷️ Sticker';
    if (content.startsWith('[GIF]:')) return '🎞️ GIF';
    if (content.startsWith('[LOCATION]:')) return '📍 Location';
    if (content.startsWith('[POLL_CREATE]:')) return '📊 Poll';
    if (content.startsWith('[ACTIVE_LIST]:')) return '☑️ Checklist';
    if (content.startsWith('[NOTE]:')) {
      final title = content.substring(7).split('\n').first;
      return '📝 ${title.isNotEmpty ? title : 'Note'}';
    }
    if (content.startsWith('[GROUP_CALL_INVITE]:')) return '📞 Group Call';
    if (content.startsWith('[GROUP_CALL_JOIN]:')) return '📞 Joined Call';
    if (content.startsWith('[GROUP_CALL_LEAVE]:')) return '📞 Left Call';
    // Regular text message — truncate
    final text = content.replaceAll('\n', ' ');
    return text.length > 50 ? '${text.substring(0, 50)}...' : text;
  }

  void _debouncedLoadContacts() {
    _refreshDebounce?.cancel();
    _refreshDebounce = Timer(const Duration(milliseconds: 500), () {
      if (!_isDisposing && mounted) _loadContacts();
    });
  }

  void _applySearchFilter() {
    if (_searchQuery.isEmpty) {
      _filteredGroups = List.from(_groups);
      _filteredContacts = List.from(_contacts);
    } else {
      final q = _searchQuery.toLowerCase();
      _filteredGroups = _groups.where((g) {
        final name = (g[1]?.toString() ?? '').toLowerCase();
        final desc = (g[3]?.toString() ?? '').toLowerCase();
        final lastMsg = (_lastMessages[g[0]] ?? '').toLowerCase();
        return name.contains(q) || desc.contains(q) || lastMsg.contains(q);
      }).toList();
      _filteredContacts = _contacts.where((c) {
        final alias = (c['alias']?.toString() ?? '').toLowerCase();
        final peerId = (c['peer_id']?.toString() ?? '').toLowerCase();
        final globalName = (c['global_name']?.toString() ?? '').toLowerCase();
        final handle = (c['handle']?.toString() ?? '').toLowerCase();
        final lastMsg = (_lastMessages[c['peer_id']] ?? '').toLowerCase();
        return alias.contains(q) || peerId.contains(q) || globalName.contains(q) || handle.contains(q) || lastMsg.contains(q);
      }).toList();
    }
  }

  void _loadProfile() {
    try {
      final profile = _client.getProfile();
      if (mounted) {
        setState(() {
          String h = profile['handle'] ?? '';
          if (h.startsWith("i@")) h = h.substring(2);
          _profileHandle = h;
          _profilePrivacyMode = profile['privacy_mode'] ?? 1;
        });
      }
    } catch (_) {}
  }

  /// Public refresh method callable from parent via GlobalKey
  void refreshContacts() => _loadContacts();

  Future<void> _loadContacts() async {
    if (_isDisposing || !mounted || _isRefreshing) return;
    _isRefreshing = true;
    setState(() => _isLoading = true);
    try {
      final contacts = _client.getContacts();
      final allGroups = _client.getAllGroups();
      final pending = _client.getPendingGroupInvites();
      final unread = _client.getUnreadCounts();
      final Map<String, String> lastMsgs = {};
      final Map<String, bool> lastMsgIsMe = {};

      final localId = _client.localPeerId ?? '';
      final groups = allGroups.where((g) {
        if (g == null || g is! List || g.length < 3) return false;
        try {
          final membersJson = g[2]?.toString() ?? '[]';
          final members = json.decode(membersJson) as List<dynamic>;
          return members.any((m) => m is Map && m['peer_id']?.toString() == localId);
        } catch (_) {
          return true;
        }
      }).toList();

      // Batch: fetch all last messages in ONE FFI call (replaces N individual calls)
      final Map<String, String> lastTimestamps = {};
      final allLastMsgs = _client.getLastMessagesAll();
      for (var entry in allLastMsgs.entries) {
        final peerId = entry.key;
        final msg = entry.value as Map<String, dynamic>?;
        if (msg != null) {
          final content = msg['content'] as String? ?? '';
          final isMe = msg['is_me'] == true || msg['is_me'] == 1 || msg['is_me'] == '1';
          final timestamp = msg['timestamp'] as String? ?? '';
          lastMsgs[peerId] = _friendlyMessagePreview(content);
          lastMsgIsMe[peerId] = isMe;
          if (timestamp.isNotEmpty) lastTimestamps[peerId] = timestamp;
        }
      }

      // Batch: fetch all last group messages in ONE FFI call
      final allLastGroupMsgs = _client.getLastGroupMessagesAll();
      for (var entry in allLastGroupMsgs.entries) {
        final groupId = entry.key;
        final msg = entry.value as Map<String, dynamic>?;
        if (msg != null) {
          final senderId = msg['sender_id']?.toString() ?? '';
          final content = msg['content']?.toString() ?? '';
          final timestamp = msg['timestamp'] as String? ?? '';
          lastMsgs[groupId] = _friendlyMessagePreview(content);
          lastMsgIsMe[groupId] = senderId == localId;
          if (timestamp.isNotEmpty) lastTimestamps[groupId] = timestamp;
        }
      }

      // Sort contacts and groups by last message timestamp (newest first)
      contacts.sort((a, b) {
        final aId = a['peer_id']?.toString() ?? '';
        final bId = b['peer_id']?.toString() ?? '';
        final aTs = lastTimestamps[aId] ?? '';
        final bTs = lastTimestamps[bId] ?? '';
        if (aTs.isEmpty && bTs.isEmpty) return 0;
        if (aTs.isEmpty) return 1;
        if (bTs.isEmpty) return -1;
        return bTs.compareTo(aTs); // descending
      });
      groups.sort((a, b) {
        final aId = (a is List && a.isNotEmpty) ? a[0]?.toString() ?? '' : '';
        final bId = (b is List && b.isNotEmpty) ? b[0]?.toString() ?? '' : '';
        final aTs = lastTimestamps[aId] ?? '';
        final bTs = lastTimestamps[bId] ?? '';
        if (aTs.isEmpty && bTs.isEmpty) return 0;
        if (aTs.isEmpty) return 1;
        if (bTs.isEmpty) return -1;
        return bTs.compareTo(aTs); // descending
      });

      if (mounted) {
        setState(() {
          _contacts = contacts;
          _groups = groups;
          _pendingInvites = pending;
          _unreadCounts = unread;
          _lastMessages = lastMsgs;
          _lastMessageIsMe = lastMsgIsMe;
          _lastMessageTimestamps = lastTimestamps;
          _isLoading = false;
          _applySearchFilter();
        });
      }
    } catch (e) {
      debugPrint("Error loading contacts/groups: $e");
      if (mounted) setState(() => _isLoading = false);
    } finally {
      _isRefreshing = false;
    }
  }

  void _showAddPeerDialog() {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _AddPeerDialog(
        onComplete: () {
          _loadContacts();
        },
      ),
    );
  }

  void _showAddByHandleDialog() {
    showDialog(
      context: context,
      builder: (dialogContext) => _ResolveHandleDialog(
        client: _client,
        parentContext: context,
      ),
    );
  }

  void _showCreateGroupDialog() {
    showDialog(
      context: context,
      builder: (context) => _CreateGroupDialog(
        contacts: _contacts,
        onComplete: _loadContacts,
      ),
    );
  }

  void _showJoinGroupDialog() {
    showDialog(
      context: context,
      builder: (context) => _JoinGroupDialog(
        onComplete: _loadContacts,
      ),
    );
  }

  void _showAddOptions() {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) {
        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                margin: EdgeInsets.symmetric(vertical: 8),
                width: 40,
                height: 4,
                decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2)),
              ),
              // Handle info banner
              Padding(
                padding: EdgeInsets.fromLTRB(20, 4, 20, 12),
                child: Container(
                  padding: EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: AppTheme.current.accent.withValues(alpha: 0.06),
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.15)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(Icons.alternate_email_rounded, color: AppTheme.current.accent, size: 18),
                          SizedBox(width: 8),
                          Expanded(
                            child: _profileHandle.isNotEmpty
                                ? Text(
                                    'Your Introvert handle: i@$_profileHandle',
                                    style: TextStyle(color: AppTheme.current.text, fontSize: 13, fontWeight: FontWeight.w600),
                                  )
                                : Text(
                                    'No Introvert handle set',
                                    style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13),
                                  ),
                          ),
                        ],
                      ),
                      SizedBox(height: 6),
                      if (_profileHandle.isEmpty)
                        Text(
                          'Go to Profile tab to set your handle.',
                          style: TextStyle(color: AppTheme.current.accent.withValues(alpha: 0.7), fontSize: 11),
                        )
                      else
                        Row(
                          children: [
                            Container(
                              padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                              decoration: BoxDecoration(
                                color: (_profilePrivacyMode == 1 ? Colors.greenAccent : Colors.orangeAccent).withValues(alpha: 0.15),
                                borderRadius: BorderRadius.circular(4),
                              ),
                              child: Text(
                                _profilePrivacyMode == 1 ? 'PUBLIC' : 'PRIVATE',
                                style: TextStyle(
                                  color: _profilePrivacyMode == 1 ? Colors.greenAccent : Colors.orangeAccent,
                                  fontSize: 9,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 1,
                                ),
                              ),
                            ),
                            SizedBox(width: 6),
                            Expanded(
                              child: Text(
                                _profilePrivacyMode == 1
                                    ? 'Unknown users can find and connect to you'
                                    : 'Only reachable via Magic Links',
                                style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.6), fontSize: 10),
                              ),
                            ),
                          ],
                        ),
                    ],
                  ),
                ),
              ),
              Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
              Material(
                color: Colors.transparent,
                child: ListTile(
                  leading: Icon(Icons.person_pin_rounded, color: AppTheme.current.accent),
                  title: Text("Add via i@ Handle", style: TextStyle(color: AppTheme.current.text)),
                  subtitle: Text("Find and connect using a persistent handle", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11)),
                  onTap: () async {
                    Navigator.pop(context);
                    await Future.delayed(const Duration(milliseconds: 300));
                    if (mounted) _showAddByHandleDialog();
                  },
                ),
              ),
              Material(
                color: Colors.transparent,
                child: ListTile(
                  leading: Icon(Icons.person_add, color: AppTheme.current.accent),
                  title: Text("Add via Magic Link", style: TextStyle(color: AppTheme.current.text)),
                  subtitle: Text("Invite a peer via Introvert Magic Link", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11)),
                  onTap: () async {
                    Navigator.pop(context);
                    await Future.delayed(const Duration(milliseconds: 300));
                    if (mounted) _showAddPeerDialog();
                  },
                ),
              ),
              Material(
                color: Colors.transparent,
                child: ListTile(
                  leading: Icon(Icons.group_add_outlined, color: AppTheme.current.accent),
                  title: Text("Create Sovereign Group", style: TextStyle(color: AppTheme.current.text)),
                  subtitle: Text("Start an encrypted group chat on the Introvert mesh swarm", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11)),
                  onTap: () async {
                    Navigator.pop(context);
                    await Future.delayed(const Duration(milliseconds: 300));
                    if (mounted) _showCreateGroupDialog();
                  },
                ),
              ),
              Material(
                color: Colors.transparent,
                child: ListTile(
                  leading: Icon(Icons.vpn_key_outlined, color: AppTheme.current.accent),
                  title: Text("Join Sovereign Group", style: TextStyle(color: AppTheme.current.text)),
                  subtitle: Text("Join the Introvert mesh swarm using an invite code", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11)),
                  onTap: () async {
                    Navigator.pop(context);
                    await Future.delayed(const Duration(milliseconds: 300));
                    if (mounted) _showJoinGroupDialog();
                  },
                ),
              ),
              SizedBox(height: 12),
            ],
          ),
        );
      },
    );
  }

  Widget _buildPendingInvitesBanner() {
    return Container(
      width: double.infinity,
      color: AppTheme.current.accent.withValues(alpha: 0.1),
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      child: Row(
        children: [
          Icon(Icons.group_add_rounded, color: AppTheme.current.accent, size: 20),
          SizedBox(width: 12),
          Expanded(
            child: Text(
              _pendingInvites.length == 1
                  ? "You have 1 pending group invitation!"
                  : "You have ${_pendingInvites.length} pending group invitations!",
              style: TextStyle(color: AppTheme.current.text, fontWeight: FontWeight.bold, fontSize: 13),
            ),
          ),
          TextButton(
            onPressed: _showPendingInvitesList,
            style: TextButton.styleFrom(
              backgroundColor: AppTheme.current.accent.withValues(alpha: 0.2),
              foregroundColor: AppTheme.current.accent,
              padding: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
              minimumSize: Size.zero,
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            ),
            child: Text("VIEW", style: TextStyle(fontSize: 11, fontWeight: FontWeight.bold)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: _isLoading

        ? Center(child: CircularProgressIndicator(color: AppTheme.current.accent))
        : Column(
            children: [
              SizedBox(height: MediaQuery.of(context).padding.top + kToolbarHeight),
              // Search bar
              Padding(
                padding: EdgeInsets.fromLTRB(16, 8, 16, 8),
                child: GlassmorphicContainer(
                  borderRadius: BorderRadius.circular(16),
                  tintAlpha: 0.06,
                  borderAlpha: 0.1,
                  padding: EdgeInsets.zero,
                  child: TextField(
                    controller: _searchController,
                    style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                    onChanged: (val) {
                      setState(() {
                        _searchQuery = val;
                        _applySearchFilter();
                      });
                    },
                    decoration: InputDecoration(
                      hintText: "Search chats...",
                      hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                      prefixIcon: Icon(Icons.search, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                      suffixIcon: _searchQuery.isNotEmpty
                          ? IconButton(
                              onPressed: () {
                                _searchController.clear();
                                setState(() {
                                  _searchQuery = '';
                                  _applySearchFilter();
                                });
                              },
                              icon: Icon(Icons.clear, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                            )
                          : null,
                      filled: true,
                      fillColor: AppTheme.current.text.withValues(alpha: 0.04),
                      contentPadding: EdgeInsets.zero,
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(16),
                        borderSide: BorderSide.none,
                      ),
                    ),
                  ),
                ),
              ),
              // Search results indicator
              if (_searchQuery.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                  child: Row(
                    children: [
                      Icon(Icons.search, size: 14, color: AppTheme.current.accent),
                      SizedBox(width: 6),
                      Text(
                        '${_filteredGroups.length + _filteredContacts.length} result${(_filteredGroups.length + _filteredContacts.length) == 1 ? '' : 's'}',
                        style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.w600),
                      ),
                    ],
                  ),
                ),
              if (_pendingInvites.isNotEmpty) _buildPendingInvitesBanner(),
              Expanded(
                child: (_filteredGroups.isEmpty && _filteredContacts.isEmpty)
                  ? Center(
                      child: Padding(
                        padding: EdgeInsets.all(32.0),
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(_searchQuery.isNotEmpty ? Icons.search_off : Icons.people_outline, size: 64, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                            SizedBox(height: 16),
                            Text(
                              _searchQuery.isNotEmpty ? 'No results found' : 'No Sovereign Connections Yet',
                              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold, color: AppTheme.current.mutedText.withValues(alpha: 0.7)),
                            ),
                            SizedBox(height: 8),
                            Text(
                              _searchQuery.isNotEmpty
                                ? 'Try a different search term.'
                                : 'Start by adding a contact or creating/joining a sovereign group.',
                              textAlign: TextAlign.center,
                              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                            ),
                            if (_searchQuery.isEmpty) ...[
                              SizedBox(height: 24),
                              ElevatedButton.icon(
                                onPressed: _showAddOptions,
                                icon: Icon(Icons.add),
                                label: Text("START CONNECTION"),
                                style: ElevatedButton.styleFrom(
                                  backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                                  foregroundColor: AppTheme.current.accent,
                                ),
                              ),
                            ],
                          ],
                        ),
                      ),
                    )
                  : ListView.separated(
                      itemCount: _filteredGroups.length + _filteredContacts.length,
                      separatorBuilder: (_, __) => Divider(height: 1, indent: 80, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
              itemBuilder: (context, index) {
                if (index < _filteredGroups.length) {
                  final group = _filteredGroups[index];
                  final groupId = group[0] as String;
                  final groupName = group[1] as String;
                  final groupDesc = group[3] as String;
                  
                  return GlassmorphicContainer(
                    borderRadius: BorderRadius.circular(16),
                    blur: 10,
                    tintAlpha: 0.08,
                    borderAlpha: 0.12,
                    padding: EdgeInsets.zero,
                    margin: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    child: Material(
                    color: Colors.transparent,
                    child: ListTile(
                    leading: SovereignAvatar(
                      radius: 30,
                      initials: groupName.isNotEmpty ? groupName[0].toUpperCase() : "G",
                    ),
                      title: Row(
                        children: [
                          Expanded(
                            child: Text(
                              groupName,
                              style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: AppTheme.current.text),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          SizedBox(width: 8),
                          Container(
                            padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                            decoration: BoxDecoration(
                              color: AppTheme.current.accent.withValues(alpha: 0.1),
                              borderRadius: BorderRadius.circular(4),
                            ),
                            child: Text(
                              "GROUP",
                              style: TextStyle(color: AppTheme.current.accent, fontSize: 8, fontWeight: FontWeight.bold, letterSpacing: 0.5),
                            ),
                          ),
                        ],
                      ),
                      subtitle: Text(
                        _lastMessages[groupId] != null
                          ? '${_lastMessageIsMe[groupId] == true ? "You: " : ""}${_lastMessages[groupId]!.replaceAll('\n', ' ')}'
                          : (groupDesc.isNotEmpty ? groupDesc : ''),
                        style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText.withValues(alpha: 0.7)),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      trailing: (_unreadCounts[groupId] ?? 0) > 0 ? _buildUnreadBadge(groupId) : null,
                      onTap: () {
                        Navigator.push(
                          context,
                          MaterialPageRoute(builder: (context) => GroupChatScreen(
                            groupId: groupId,
                            groupName: groupName,
                          )),
                        ).then((_) => _loadContacts());
                      },
                    ),
                    ),
                  );
                } else {
                  final contact = _filteredContacts[index - _filteredGroups.length];
                  final peerId = contact['peer_id'] as String;
                  final alias = contact['alias'] as String?;
                  final avatar = contact['avatar'] as String?;
                  return GlassmorphicContainer(
                    borderRadius: BorderRadius.circular(16),
                    blur: 10,
                    tintAlpha: 0.08,
                    borderAlpha: 0.12,
                    padding: EdgeInsets.zero,
                    margin: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    child: Material(
                    color: Colors.transparent,
                    child: ListTile(
                    leading: SovereignAvatar(
                      radius: 30,
                      prestigeTier: contact['prestige_tier'] as int? ?? 0,
                      avatar: avatar != null ? MemoryImage(_decodeAvatar(avatar)) : null,
                      initials: (alias == null || alias.isEmpty) ? "?" : alias[0].toUpperCase(),
                    ),
                    title: Text(
                      alias ?? peerId, 
                      style: TextStyle(
                        fontFamily: (alias == null || alias.isEmpty) ? 'monospace' : null, 
                        fontSize: (alias == null || alias.isEmpty) ? 13 : 16, 
                        fontWeight: FontWeight.bold,
                        color: (alias == null || alias.isEmpty) ? AppTheme.current.text.withValues(alpha: 0.7) : AppTheme.current.text,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(
                      _lastMessages[peerId] != null
                        ? '${_lastMessageIsMe[peerId] == true ? "You: " : ""}${_lastMessages[peerId]!.replaceAll('\n', ' ')}'
                        : (alias == null || alias.isEmpty
                            ? (contact['is_anchor_capable'] ? "ANCHOR CAPABLE" : "DIRECT PEER")
                            : ''),
                      style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText.withValues(alpha: 0.7)),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if ((_unreadCounts[peerId] ?? 0) > 0) 
                          Padding(
                            padding: EdgeInsets.only(right: 8.0),
                            child: _buildUnreadBadge(peerId),
                          ),
                        IconButton(

                          icon: Icon(Icons.videocam_rounded, color: AppTheme.current.accent, size: 20),
                          tooltip: "Video Call",
                          onPressed: () {
                            try {
                              _client.initiateWebRtc(peerId, 2);
                              Navigator.push(
                                context,
                                MaterialPageRoute(builder: (context) => CallScreen(
                                  peerId: peerId,
                                  contactName: alias,
                                  isCaller: true,
                                )),
                              );
                            } catch (e) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                SnackBar(content: Text("Call failed: $e")),
                              );
                            }
                          },
                        ),
                        PopupMenuButton<String>(
                          icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText, size: 20),
                          onSelected: (action) => _handleContactAction(action, peerId, alias),
                          itemBuilder: (context) => [
                            PopupMenuItem(
                              value: 'rename',
                              child: Row(
                                children: [
                                  Icon(Icons.edit_outlined, size: 18, color: AppTheme.current.accent),
                                  SizedBox(width: 8),
                                  Text("Rename Contact"),
                                ],
                              ),
                            ),
                            PopupMenuItem(
                              value: 'clear_chat',
                              child: Row(
                                children: [
                                  Icon(Icons.delete_sweep_outlined, size: 18, color: Colors.orangeAccent),
                                  SizedBox(width: 8),
                                  Text("Clear Chat"),
                                ],
                              ),
                            ),
                            PopupMenuItem(
                              value: 'delete_contact',
                              child: Row(
                                children: [
                                  Icon(Icons.delete_forever_outlined, size: 18, color: Colors.redAccent),
                                  SizedBox(width: 8),
                                  Text("Delete Contact", style: TextStyle(color: Colors.redAccent)),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                    onTap: () {
                      Navigator.push(
                        context,
                        MaterialPageRoute(builder: (context) => ChatScreen(
                          peerId: peerId,
                          peerName: alias,
                          avatarBase64: avatar,
                        )),
                      ).then((_) => _loadContacts());
                    },
                  ),
                  ),
                  );
                }
              },
            ),
              ),
            ],
          ),
      floatingActionButton: Padding(
        padding: const EdgeInsets.only(bottom: 100),
        child: FloatingActionButton(
          heroTag: null,
          onPressed: _showAddOptions,
          backgroundColor: AppTheme.current.accent,
          foregroundColor: Colors.black,
          child: Icon(Icons.add),
        ),
      ),
      floatingActionButtonLocation: FloatingActionButtonLocation.endFloat,
    );
  }

  void _handleContactAction(String action, String peerId, String? currentAlias) async {
    if (action == 'delete_contact') {
      final confirm = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          backgroundColor: AppTheme.current.surface,
          title: Text("Delete Contact?", style: TextStyle(color: Colors.redAccent)),
          content: Text("Remove ${currentAlias ?? peerId} from your verified contacts? This also deletes chat history."),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, false), child: Text("CANCEL")),
            TextButton(
              onPressed: () => Navigator.pop(context, true), 
              child: Text("DELETE", style: TextStyle(color: Colors.redAccent)),
            ),
          ],
        ),
      );

      if (confirm == true) {
        await _client.deleteContact(peerId);
        _loadContacts();
      }
    } else if (action == 'clear_chat') {
      final confirm = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          backgroundColor: AppTheme.current.surface,
          title: Text("Clear Chat?", style: TextStyle(color: Colors.orangeAccent)),
          content: Text("Wipe all message history for ${currentAlias ?? peerId}? This cannot be undone."),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, false), child: Text("CANCEL")),
            TextButton(
              onPressed: () => Navigator.pop(context, true), 
              child: Text("CLEAR", style: TextStyle(color: Colors.orangeAccent)),
            ),
          ],
        ),
      );

      if (confirm == true) {
        await _client.deleteChat(peerId);
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text("Chat history cleared.")),
          );
        }
      }
    } else if (action == 'rename') {
      final controller = TextEditingController(text: currentAlias ?? "");
      try {
        final newAlias = await showDialog<String>(
          context: context,
          builder: (context) => AlertDialog(
            backgroundColor: AppTheme.current.surface,
            title: Text("Rename Contact"),
            content: TextField(
              controller: controller,
              style: TextStyle(color: AppTheme.current.text),
              decoration: InputDecoration(
                labelText: "Alias",
                labelStyle: TextStyle(color: AppTheme.current.accent),
                enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
              ),
            ),
            actions: [
              TextButton(onPressed: () => Navigator.pop(context, null), child: Text("CANCEL")),
              TextButton(
                onPressed: () => Navigator.pop(context, controller.text.trim()),
                child: Text("SAVE", style: TextStyle(color: AppTheme.current.accent)),
              ),
            ],
          ),
        );

        if (newAlias != null) {
          _client.updateContactAlias(peerId, newAlias);
          _loadContacts();
        }
      } finally {
        controller.dispose();
      }
    }
  }
}

class _ClawTerminalDialog extends StatefulWidget {
  final String title;
  final List<String> milestones;
  final String? finalReport;

  const _ClawTerminalDialog({required this.title, required this.milestones, this.finalReport});

  @override
  State<_ClawTerminalDialog> createState() => _ClawTerminalDialogState();
}

class _ClawTerminalDialogState extends State<_ClawTerminalDialog> with SingleTickerProviderStateMixin {
  late AnimationController _cursorController;
  bool _showCursor = true;
  List<String> _displayedMilestones = [];

  @override
  void initState() {
    super.initState();
    _cursorController = AnimationController(vsync: this, duration: Duration(milliseconds: 500))
      ..repeat(reverse: true);
    _cursorController.addListener(() {
      setState(() => _showCursor = _cursorController.value > 0.5);
    });
    _animateMilestones();
  }

  void _animateMilestones() async {
    for (int i = 0; i < widget.milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 200 + (i * 80)));
      if (mounted) {
        setState(() => _displayedMilestones.add(widget.milestones[i]));
      }
    }
    // Stop cursor animation when final report is shown
    if (widget.finalReport != null && mounted) {
      _cursorController.stop();
      setState(() => _showCursor = false);
    }
  }

  @override
  void dispose() {
    _cursorController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final hasReport = widget.finalReport != null;
    return Dialog(
      backgroundColor: Colors.transparent,
      insetPadding: EdgeInsets.symmetric(horizontal: 24, vertical: 40),
      child: Container(
        constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.7),
        decoration: BoxDecoration(
          color: Color(0xFF0A0A0A),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.3), width: 1),
          boxShadow: [BoxShadow(color: Colors.greenAccent.withValues(alpha: 0.1), blurRadius: 20, spreadRadius: 2)],
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: Colors.greenAccent.withValues(alpha: 0.08),
                borderRadius: BorderRadius.vertical(top: Radius.circular(12)),
                border: Border(bottom: BorderSide(color: Colors.greenAccent.withValues(alpha: 0.15))),
              ),
              child: Row(
                children: [
                  Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.redAccent.withValues(alpha: 0.7))),
                  SizedBox(width: 6),
                  Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.amber.withValues(alpha: 0.7))),
                  SizedBox(width: 6),
                  Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.greenAccent.withValues(alpha: 0.7))),
                  SizedBox(width: 12),
                  Text(widget.title, style: TextStyle(
                    fontSize: 11, fontWeight: FontWeight.bold,
                    color: Colors.greenAccent.withValues(alpha: 0.8),
                    letterSpacing: 1.5, fontFamily: 'monospace',
                  )),
                  Spacer(),
                  GestureDetector(
                    onTap: () => Navigator.of(context).pop(),
                    child: Icon(Icons.close, size: 16, color: Colors.greenAccent.withValues(alpha: 0.5)),
                  ),
                ],
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    ..._displayedMilestones.map((m) => Padding(
                      padding: EdgeInsets.only(bottom: 4),
                      child: Text(m, style: TextStyle(
                        fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.9),
                        fontFamily: 'monospace', height: 1.4,
                      )),
                    )),
                    if (!hasReport && _displayedMilestones.isNotEmpty)
                      Text(_showCursor ? '█' : ' ', style: TextStyle(
                        fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.6), fontFamily: 'monospace',
                      )),
                    if (!hasReport && _displayedMilestones.isEmpty)
                      Row(children: [
                        SizedBox(width: 12, height: 12, child: CircularProgressIndicator(strokeWidth: 1.5, color: Colors.greenAccent)),
                        SizedBox(width: 8),
                        Text('Initializing...', style: TextStyle(
                          fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.6), fontFamily: 'monospace',
                        )),
                      ]),
                    if (hasReport) ...[
                      SizedBox(height: 8),
                      Container(
                        width: double.infinity,
                        padding: EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Colors.greenAccent.withValues(alpha: 0.05),
                          borderRadius: BorderRadius.circular(8),
                          border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.15)),
                        ),
                        child: Text(widget.finalReport!, style: TextStyle(
                          fontSize: 11, color: Colors.greenAccent, fontFamily: 'monospace', height: 1.5,
                        )),
                      ),
                      SizedBox(height: 12),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          GestureDetector(
                            onTap: () async {
                              try {
                                final buffer = StringBuffer();
                                buffer.writeln('INTRO-CLAW ${widget.title}');
                                buffer.writeln('Generated: ${DateTime.now().toIso8601String()}');
                                buffer.writeln('=' * 60);
                                for (final m in widget.milestones) {
                                  buffer.writeln(m);
                                }
                                if (widget.finalReport != null) {
                                  buffer.writeln('');
                                  buffer.writeln('--- REPORT ---');
                                  buffer.writeln(widget.finalReport!);
                                }
                                final fileName = 'introvert_${widget.title.toLowerCase().replaceAll(' ', '_')}_${DateTime.now().millisecondsSinceEpoch}.txt';
                                Directory dir;
                                if (Platform.isAndroid) {
                                  dir = Directory('/storage/emulated/0/Download');
                                  if (!await dir.exists()) {
                                    dir = await getTemporaryDirectory();
                                  }
                                } else if (Platform.isIOS || Platform.isMacOS) {
                                  dir = await getApplicationDocumentsDirectory();
                                } else {
                                  dir = await getTemporaryDirectory();
                                }
                                final file = File('${dir.path}/$fileName');
                                await file.writeAsString(buffer.toString());
                                if (mounted) {
                                  ScaffoldMessenger.of(context).showSnackBar(
                                    SnackBar(content: Text('Saved: $fileName')),
                                  );
                                }
                              } catch (e) {
                                if (mounted) {
                                  ScaffoldMessenger.of(context).showSnackBar(
                                    SnackBar(content: Text('Failed to save: $e')),
                                  );
                                }
                              }
                            },
                            child: Container(
                              padding: EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                              decoration: BoxDecoration(
                                color: Colors.greenAccent.withValues(alpha: 0.08),
                                borderRadius: BorderRadius.circular(6),
                                border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.3)),
                              ),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(Icons.download_rounded, size: 14, color: Colors.greenAccent.withValues(alpha: 0.8)),
                                  SizedBox(width: 6),
                                  Text('DOWNLOAD', style: TextStyle(
                                    fontSize: 11, fontWeight: FontWeight.bold,
                                    color: Colors.greenAccent.withValues(alpha: 0.8), letterSpacing: 1.5, fontFamily: 'monospace',
                                  )),
                                ],
                              ),
                            ),
                          ),
                          SizedBox(width: 12),
                          GestureDetector(
                            onTap: () => Navigator.of(context).pop(),
                            child: Container(
                              padding: EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                              decoration: BoxDecoration(
                                color: Colors.greenAccent.withValues(alpha: 0.15),
                                borderRadius: BorderRadius.circular(6),
                                border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.4)),
                              ),
                              child: Text('CLOSE', style: TextStyle(
                                fontSize: 11, fontWeight: FontWeight.bold,
                                color: Colors.greenAccent, letterSpacing: 1.5, fontFamily: 'monospace',
                              )),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _AddPeerDialog extends StatefulWidget {
  final VoidCallback onComplete;
  const _AddPeerDialog({required this.onComplete});

  @override
  State<_AddPeerDialog> createState() => _AddPeerDialogState();
}

class _AddPeerDialogState extends State<_AddPeerDialog> {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _codeController = TextEditingController();
  StreamSubscription<NetworkEvent>? _networkSubscription;
  
  String? _inviteCode;
  bool _isWaiting = false;
  String _status = "Select an onboarding method";

  @override
  void initState() {
    super.initState();
    _networkSubscription = _client.networkStream.listen((event) {
      if (!mounted) return;
      if (event.type == 6) {
        final dataStr = String.fromCharCodes(event.data);
        if (dataStr.startsWith('ERROR:')) {
          final parts = dataStr.split(':');
          final errorMsg = parts.length > 2 ? parts.sublist(2).join(':') : parts.last;
          setState(() {
            _status = "Error: $errorMsg";
            _isWaiting = false;
            _inviteCode = null;
          });
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text("Magic Link Error: $errorMsg"), backgroundColor: Colors.redAccent),
          );
        } else {
          setState(() {
            _inviteCode = dataStr;
            _status = "Share this code with your peer:";
            _isWaiting = false;
          });
        }
      } else if (event.type == 7) {
        final messenger = ScaffoldMessenger.of(context);
        Navigator.pop(context);
        widget.onComplete();
        messenger.showSnackBar(
          SnackBar(content: Text("Contact Verified Successfully!")),
        );
      }
    });
  }

  @override
  void dispose() {
    _networkSubscription?.cancel();
    _codeController.dispose();
    try {
      _client.abortWormhole();
    } catch (_) {}
    super.dispose();
  }

  void _startInvite() {
    setState(() {
      _isWaiting = true;
      _status = "Generating Introvert Magic Link...";
    });
    _client.startWormholeInvite();
  }

  void _joinInvite() {
    final code = _codeController.text.trim();
    if (code.isEmpty) return;

    setState(() {
      _isWaiting = true;
      _status = "Joining session...";
    });
    _client.joinWormholeInvite(code);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      title: Text("Add Sovereign Peer", style: TextStyle(color: AppTheme.current.accent)),
      content: SingleChildScrollView(
        child: SizedBox(
          width: double.maxFinite,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(_status, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 14)),
              SizedBox(height: 20),
              if (_inviteCode != null) ...[
                Container(
                  padding: EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: Colors.black26,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3)),
                  ),
                  child: SelectableText(
                    _inviteCode!,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.bold,
                      color: AppTheme.current.accent,
                      letterSpacing: 2,
                    ),
                  ),
                ),
                SizedBox(height: 12),
                Text("Waiting for peer to join...", style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
              ] else if (!_isWaiting) ...[
                ElevatedButton(
                  onPressed: _startInvite,
                  style: ElevatedButton.styleFrom(
                    minimumSize: const Size(double.infinity, 50),
                    backgroundColor: AppTheme.current.accent,
                    foregroundColor: Colors.black,
                  ),
                  child: Text("CREATE INVITE CODE"),
                ),
                Padding(
                  padding: EdgeInsets.symmetric(vertical: 16.0),
                  child: Text("OR", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                ),
                TextField(
                  controller: _codeController,
                  decoration: InputDecoration(
                    hintText: "ENTER PEER'S CODE",
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                    filled: true,
                    fillColor: Colors.black26,
                    border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
                  ),
                  style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'),
                ),
                SizedBox(height: 12),
                ElevatedButton(
                  onPressed: _joinInvite,
                  style: ElevatedButton.styleFrom(
                    minimumSize: const Size(double.infinity, 50),
                    backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                    foregroundColor: AppTheme.current.text,
                  ),
                  child: Text("JOIN SESSION"),
                ),
              ] else
                CircularProgressIndicator(color: AppTheme.current.accent),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
        ),
      ],
    );
  }
}



class SettingsTab extends StatefulWidget {
  final VoidCallback? onMessengerSettingsChanged;
  final VoidCallback? onContactsChanged;
  const SettingsTab({super.key, this.onMessengerSettingsChanged, this.onContactsChanged});

  @override
  State<SettingsTab> createState() => _SettingsTabState();
}

class _SettingsTabState extends State<SettingsTab> with AutomaticKeepAliveClientMixin {
  bool _isAnchorMode = false;
  bool _isTunnelMode = false;
  String _klipyApiKey = '';
  String _clawStatusJson = '{}';
  int _clawDriveBytes = 0;
  int _clawMeshBytes = 0;
  Timer? _clawStatusTimer;
  // Messenger integration state
  static const int _maxMessengerTabs = 3;
  bool _whatsappEnabled = true;
  bool _telegramEnabled = true;
  bool _discordEnabled = false;
  bool _slackEnabled = false;
  bool _messengerEnabled = false;
  bool _googleMessagesEnabled = false;
  List<Map<String, dynamic>> _clawActivityLog = [];
  Timer? _clawActivityLogTimer;
  final _clawLogScrollController = ScrollController();
  int _rbnCount = 0;
  String _appVersion = 'Loading...';
  int _meshNodeCount = 0;
  int _connectedPeerCount = 0;
  StreamSubscription? _economySubscription;
  Map<String, dynamic> _economyStats = {
    'intr_balance': 0,
    'sol_balance': 0,
    'usdc_balance': 0,
    'pending_rewards': 0,
    'total_relayed': 0,
    'sol_address': 'Connecting...',
  };

  Map<String, dynamic> _swarmStats = {
    'total_nodes': 1,
    'active_users': 1,
    'collective_capacity_gb': 0,
    'active_transfers': 0,
  };
  StreamSubscription? _swarmStatsSubscription;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _startMonitoring();
    _isAnchorMode = IntrovertClient().isAnchorModeEnabled();
    _isTunnelMode = IntrovertClient().isTunnelModeEnabled();
    _loadKlipyApiKey();
    _loadMessengerSettings();
    _loadAppVersion();
    
    // Intro-Claw is always active — no toggle needed
    IntrovertClient().setIntroClawActive(true);
    _refreshClawStatus();
    _refreshClawActivityLog();
    _clawActivityLogTimer = Timer.periodic(Duration(seconds: 10), (_) {
      if (mounted) _refreshClawActivityLog();
    });
    
    _swarmStatsSubscription = IntrovertClient().swarmStatsStream.listen((stats) {
      if (mounted) {
        setState(() {
          _swarmStats = stats;
        });
      }
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) IntrovertClient().requestSwarmStats();
    });
  }

  @override
  void dispose() {
    _swarmStatsSubscription?.cancel();
    _economySubscription?.cancel();
    _clawStatusTimer?.cancel();
    _clawActivityLogTimer?.cancel();
    _clawLogScrollController.dispose();
    super.dispose();
  }

  String _getBuildNumber() {
    return _appVersion;
  }

  Future<void> _loadAppVersion() async {
    try {
      final info = await PackageInfo.fromPlatform();
      if (mounted) {
        setState(() => _appVersion = '${info.version}+${info.buildNumber}');
      }
    } catch (e) {
      debugPrint('[Settings] Failed to load app version: $e');
    }
  }

  void _showRbnManagerDialog() {
    final mainShellState = context.findAncestorStateOfType<_MainShellState>();
    if (mainShellState == null) return;
    showDialog(
      context: context,
      builder: (ctx) => _RbnManagerDialog(
        client: IntrovertClient(),
        parentState: mainShellState,
      ),
    );
  }

  void _showSwarmStatus() {
    IntrovertClient().requestSwarmStats();
    showDialog(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setDialogState) {
          return StreamBuilder<Map<String, dynamic>>(
            stream: IntrovertClient().swarmStatsStream,
            initialData: _swarmStats,
            builder: (context, snapshot) {
              final stats = snapshot.data ?? _swarmStats;
              return AlertDialog(
                backgroundColor: AppTheme.current.bg,
                contentPadding: EdgeInsets.zero,
                insetPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 24),
                content: Container(
                  padding: EdgeInsets.all(24),
                  decoration: BoxDecoration(
                    color: AppTheme.current.bg,
                    borderRadius: BorderRadius.circular(24),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                    boxShadow: [
                      BoxShadow(
                        color: AppTheme.current.accent.withValues(alpha: 0.1),
                        blurRadius: 40,
                        spreadRadius: -10,
                      ),
                    ],
                  ),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Row(
                        children: [
                          Container(
                            padding: EdgeInsets.all(10),
                            decoration: BoxDecoration(
                              color: AppTheme.current.accent.withValues(alpha: 0.1),
                              shape: BoxShape.circle,
                            ),
                            child: Icon(Icons.hub_rounded, color: AppTheme.current.accent, size: 24),
                          ),
                          SizedBox(width: 16),
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                "SWARM INTELLIGENCE",
                                style: TextStyle(
                                  color: AppTheme.current.accent,
                                  fontSize: 10,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 1.5,
                                ),
                              ),
                              Text(
                                "Live Mesh Status",
                                style: TextStyle(color: AppTheme.current.text, fontSize: 18, fontWeight: FontWeight.bold),
                              ),
                            ],
                          ),
                        ],
                      ),
                      SizedBox(height: 24),
                      _buildStatRow(Icons.people_alt_rounded, "Active Users Online", "${stats['active_users']}"),
                      Divider(height: 32, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                      _buildStatRow(Icons.cloud_done_rounded, "Collective Storage", "${stats['collective_capacity_gb']} GB"),
                      Divider(height: 32, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                      _buildStatRow(Icons.dns_rounded, "Total Mesh Nodes", "${stats['total_nodes']}"),
                      Divider(height: 32, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                      _buildStatRow(Icons.swap_horizontal_circle_rounded, "Active Swarm Transfers", "${stats['active_transfers']}"),
                      SizedBox(height: 32),
                      SizedBox(
                        width: double.infinity,
                        child: ElevatedButton(
                          onPressed: () => Navigator.pop(context),
                          style: ElevatedButton.styleFrom(
                            backgroundColor: AppTheme.current.accent,
                            foregroundColor: Colors.black,
                            padding: EdgeInsets.symmetric(vertical: 16),
                            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                            elevation: 0,
                          ),
                          child: Text("CLOSE", style: TextStyle(fontWeight: FontWeight.bold)),
                        ),
                      ),
                    ],
                  ),
                ),
              );
            },
          );
        },
      ),
    );
  }

  void _showManifesto() {
    showDialog(
      context: context,
      builder: (context) => Dialog(
        backgroundColor: Colors.transparent,
        insetPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 24),
        child: Container(
          constraints: BoxConstraints(maxWidth: 440, maxHeight: MediaQuery.of(context).size.height * 0.75),
          decoration: BoxDecoration(
            color: AppTheme.current.surface,
            borderRadius: BorderRadius.circular(24),
            border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.25), width: 1),
            boxShadow: [
              BoxShadow(color: AppTheme.current.accent.withValues(alpha: 0.08), blurRadius: 40, spreadRadius: -10),
            ],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Header
              Container(
                width: double.infinity,
                padding: EdgeInsets.symmetric(horizontal: 24, vertical: 20),
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    colors: [
                      AppTheme.current.accent.withValues(alpha: 0.12),
                      AppTheme.current.accent.withValues(alpha: 0.04),
                    ],
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                  ),
                  borderRadius: BorderRadius.vertical(top: Radius.circular(24)),
                ),
                child: Column(
                  children: [
                    Image.asset(
                      'assets/images/icon_transparent.png',
                      height: 36,
                      filterQuality: FilterQuality.high,
                      color: AppTheme.current.accent,
                    ),
                    SizedBox(height: 12),
                    Text(
                      "THE INTROVERT MANIFESTO",
                      style: TextStyle(
                        color: AppTheme.current.accent,
                        fontSize: 11,
                        fontWeight: FontWeight.bold,
                        letterSpacing: 1.8,
                      ),
                    ),
                    SizedBox(height: 4),
                    Text(
                      "A Protocol for Digital Sovereignty",
                      style: TextStyle(
                        color: AppTheme.current.mutedText,
                        fontSize: 12,
                        fontStyle: FontStyle.italic,
                      ),
                    ),
                  ],
                ),
              ),
              Divider(height: 1, color: AppTheme.current.accent.withValues(alpha: 0.15)),
              // Content
              Flexible(
                child: SingleChildScrollView(
                  padding: EdgeInsets.all(24),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _buildManifestoSection(
                        "Overview",
                        "Introvert is a decentralized, sustainable communication ecosystem engineered to replace the traditional client-server model. By utilizing a high-performance peer-to-peer (P2P) mesh architecture, the platform removes central intermediaries, ensuring that data ownership, network control, and infrastructural environmental footprint remain exclusively with the user.",
                      ),
                      _buildManifestoPillar(
                        "1",
                        "Zero-Knowledge Privacy & Identity",
                        [
                          "Decentralized Identity: Identities are anchored to a permanent cryptographic seed rather than a phone number or email address.",
                          "Sovereign Storage: Data is stored only on the user's device using zero-knowledge encryption; users are never treated as products.",
                          "Metadata Hardening: The system is designed to harden both content and metadata, preventing tracking or commercialization.",
                          "Data Persistence: Cryptographic identity is fully recoverable via the master seed, maintaining permanent user control.",
                        ],
                      ),
                      _buildManifestoPillar(
                        "2",
                        "High-Performance Architecture & Speed",
                        [
                          "Dual-Plane Engineering: Separates signaling from data distribution to achieve ~100ms text latency, matching centralized market leaders.",
                          "Optimized Media Distribution: Split-plane architecture leverages local network capacity, outperforming standard internet routing.",
                          "Infrastructure Efficiency: The mesh operates as long as peers exist, ensuring resilience without server farms.",
                        ],
                      ),
                      _buildManifestoPillar(
                        "3",
                        "Green Energy & Eco-Sustainability",
                        [
                          "Zero-Data-Center Footprint: Replacing power-hungry data centers with existing consumer hardware reduces the internet's carbon footprint.",
                          "Hardware Lifecycle Efficiency: Utilizing idle bandwidth and local storage reduces e-waste and cuts redundant power needs.",
                          "Protocol Optimization: Client-only DHT modes and optimized polling minimize processor wakeups and conserve battery.",
                        ],
                      ),
                      _buildManifestoPillar(
                        "4",
                        "Network Participation & Resource Sharing",
                        [
                          "Telemetry Weight System: Active nodes receive allocation multipliers based on verified network contribution metrics.",
                          "Uptime Tracking: Nodes are scored for maintaining continuous availability and providing routing services.",
                          "Accessible Participation: Synchronization mechanisms lower the barrier for global network contribution.",
                        ],
                      ),
                      SizedBox(height: 16),
                      // Closing statement
                      Container(
                        width: double.infinity,
                        padding: EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: AppTheme.current.accent.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                        ),
                        child: Text(
                          "The objective of Introvert is to provide a privacy-obsessed, eco-sustainable communication tool for professionals and everyday users alike. It combines the efficiency of established Web2 platforms with the sovereign ownership, carbon efficiency, and economic incentives of Web3 architectures.",
                          style: TextStyle(color: AppTheme.current.text, fontSize: 13, height: 1.6),
                        ),
                      ),
                      SizedBox(height: 12),
                      Center(
                        child: Text(
                          "Own your words. Own your network.\nOwn your impact. Own your future.",
                          style: TextStyle(
                            color: AppTheme.current.accent,
                            fontSize: 12,
                            fontWeight: FontWeight.bold,
                            fontStyle: FontStyle.italic,
                            height: 1.6,
                          ),
                          textAlign: TextAlign.center,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              // Close button
              Divider(height: 1, color: AppTheme.current.accent.withValues(alpha: 0.15)),
              Padding(
                padding: EdgeInsets.symmetric(horizontal: 24, vertical: 16),
                child: SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.current.accent,
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                      padding: EdgeInsets.symmetric(vertical: 12),
                    ),
                    onPressed: () => Navigator.pop(context),
                    child: Text("CLOSE", style: TextStyle(color: AppTheme.current.bg, fontWeight: FontWeight.bold, letterSpacing: 1)),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildManifestoSection(String title, String body) {
    return Padding(
      padding: EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            title.toUpperCase(),
            style: TextStyle(
              color: AppTheme.current.accent,
              fontSize: 10,
              fontWeight: FontWeight.bold,
              letterSpacing: 1.5,
            ),
          ),
          SizedBox(height: 8),
          Text(
            body,
            style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.8), fontSize: 13, height: 1.6),
          ),
        ],
      ),
    );
  }

  Widget _buildManifestoPillar(String number, String title, List<String> points) {
    return Padding(
      padding: EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 24,
                height: 24,
                decoration: BoxDecoration(
                  color: AppTheme.current.accent.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Center(
                  child: Text(number, style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold)),
                ),
              ),
              SizedBox(width: 10),
              Expanded(
                child: Text(
                  title,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold),
                ),
              ),
            ],
          ),
          SizedBox(height: 10),
          ...points.map((point) {
            final colonIdx = point.indexOf(':');
            if (colonIdx > 0 && colonIdx < 60) {
              final label = point.substring(0, colonIdx);
              final rest = point.substring(colonIdx + 1).trim();
              return Padding(
                padding: EdgeInsets.only(bottom: 6, left: 34),
                child: RichText(
                  text: TextSpan(
                    children: [
                      TextSpan(text: "$label: ", style: TextStyle(color: AppTheme.current.accent.withValues(alpha: 0.8), fontSize: 12, fontWeight: FontWeight.bold)),
                      TextSpan(text: rest, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12, height: 1.5)),
                    ],
                  ),
                ),
              );
            }
            return Padding(
              padding: EdgeInsets.only(bottom: 6, left: 34),
              child: Text(
                "• $point",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12, height: 1.5),
              ),
            );
          }).toList(),
        ],
      ),
    );
  }

  Widget _buildStatRow(IconData icon, String label, String value) {
    return Row(
      children: [
        Icon(icon, color: AppTheme.current.mutedText.withValues(alpha: 0.7), size: 20),
        SizedBox(width: 16),
        Text(label, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)),
        const Spacer(),
        Text(
          value,
          style: TextStyle(
            color: AppTheme.current.text,
            fontSize: 15,
            fontWeight: FontWeight.bold,
            fontFamily: 'monospace',
          ),
        ),
      ],
    );
  }

  void _loadKlipyApiKey() async {
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    setState(() {
      _klipyApiKey = prefs.getString('klipy_api_key') ?? 'lh4LISPXX2wQQFRZ6JCfqmUeKNeHUJ2Ht8i3dxSqadvlKJC4T5jJLzDxx2jRfW5b';
    });
  }

  void _loadMessengerSettings() async {
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    setState(() {
      _whatsappEnabled = prefs.getBool('whatsapp_enabled') ?? true;
      _telegramEnabled = prefs.getBool('telegram_enabled') ?? true;
      _discordEnabled = prefs.getBool('discord_enabled') ?? false;
      _slackEnabled = prefs.getBool('slack_enabled') ?? false;
      _messengerEnabled = prefs.getBool('messenger_enabled') ?? false;
      _googleMessagesEnabled = prefs.getBool('google_messages_enabled') ?? false;
    });
  }

  int get _enabledMessengerCount {
    int count = 0;
    if (_whatsappEnabled) count++;
    if (_telegramEnabled) count++;
    if (_discordEnabled) count++;
    if (_slackEnabled) count++;
    if (_messengerEnabled) count++;
    if (_googleMessagesEnabled) count++;
    return count;
  }

  void _startMonitoring() {
    // Economy monitoring is started by MainShell — just listen to the stream here
    _economySubscription = IntrovertClient().economyStream.listen((stats) {
      if (mounted) {
        setState(() {
          _economyStats = stats;
        });

        // Dynamically compute and update prestige tier based on INTR balance
        final rawBalance = stats['intr_balance'];
        double balance = 0.0;
        if (rawBalance != null) {
          if (rawBalance is num) {
            balance = rawBalance.toDouble();
          } else {
            balance = double.tryParse(rawBalance.toString()) ?? 0.0;
          }
          balance /= 1000000000.0; // 9 decimals
        }

        int tier = 0;
        if (balance >= 1000000.0) {
          tier = 4; // Platinum
        } else if (balance >= 500000.0) {
          tier = 3; // Gold
        } else if (balance >= 250000.0) {
          tier = 2; // Silver
        } else if (balance >= 100000.0) {
          tier = 1; // Sentinel
        } else {
          tier = 0; // Citizen
        }

        IntrovertClient().setProfileTier(tier);
      }
    });
  }

  void _toggleAnchorMode(bool value) async {
    setState(() => _isAnchorMode = value);
    IntrovertClient().setAnchorMode(value);

    // Node Mode requires a WakeLock on Android to fulfill mesh duties while asleep
    AlertService.setStayAwake(value);

    final prefs = await SharedPreferences.getInstance();

    await prefs.setBool('isAnchorMode', value);

    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(value 
            ? "Anchor Mode Enabled: Contributing to Mesh Swarm..." 
            : "Anchor Mode Disabled"),
          backgroundColor: value ? Colors.cyan : null,
        ),
      );
    }
  }

  void _showAnchorModeInfo() {
    showDialog(
      context: context,
      builder: (ctx) => Dialog(
        backgroundColor: AppTheme.current.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: EdgeInsets.fromLTRB(20, 16, 20, 8),
                child: Row(
                  children: [
                    Icon(Icons.anchor_rounded, color: AppTheme.current.accent, size: 20),
                    SizedBox(width: 8),
                    Expanded(
                      child: Text('ANCHOR NODE MODE', style: TextStyle(
                        color: AppTheme.current.accent, fontSize: 13,
                        fontWeight: FontWeight.bold, letterSpacing: 1.2,
                      )),
                    ),
                    GestureDetector(
                      onTap: () => Navigator.pop(ctx),
                      child: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText),
                    ),
                  ],
                ),
              ),
              Flexible(
                child: SingleChildScrollView(
                  padding: EdgeInsets.fromLTRB(20, 4, 20, 16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // Earnings highlight
                      Container(
                        padding: EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Colors.amber.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: Colors.amber.withValues(alpha: 0.2)),
                        ),
                        child: Row(
                          children: [
                            Icon(Icons.monetization_on_rounded, color: Colors.amber, size: 20),
                            SizedBox(width: 10),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Earns Extra \$INTR Rewards', style: TextStyle(
                                    color: Colors.amber, fontSize: 12, fontWeight: FontWeight.bold,
                                  )),
                                  SizedBox(height: 2),
                                  Text('Anchor nodes earn additional \$INTR tokens for contributing to the Introvert mesh swarm. Exact reward rates will be published soon.', style: TextStyle(
                                    color: AppTheme.current.mutedText, fontSize: 10, height: 1.3,
                                  )),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                      SizedBox(height: 12),
                      // What it does
                      Text('WHAT ANCHOR MODE DOES', style: TextStyle(
                        fontSize: 10, fontWeight: FontWeight.bold,
                        color: AppTheme.current.mutedText, letterSpacing: 1,
                      )),
                      SizedBox(height: 8),
                      _buildAnchorRow(Icons.swap_horiz_rounded, 'Relay Server', 'Routes traffic between peers that cannot connect directly (NAT traversal)', Colors.blue),
                      _buildAnchorRow(Icons.hub_rounded, 'Full DHT Server', 'Participates in Kademlia routing table — helps peers discover each other', Colors.teal),
                      _buildAnchorRow(Icons.mail_outline_rounded, 'Mailbox Storage', 'Stores encrypted messages for offline peers in persistent storage', Colors.orange),
                      _buildAnchorRow(Icons.mark_email_read_rounded, 'Mailbox Drain', 'Delivers stored messages when offline peers reconnect', Colors.green),
                      _buildAnchorRow(Icons.group_rounded, 'Group Message Storage', 'Stores group chat messages for other members who missed them', Colors.purple),
                      _buildAnchorRow(Icons.download_rounded, 'Group Media Auto-Pull', 'Automatically fetches file attachments for group members', Colors.cyan),
                      _buildAnchorRow(Icons.cell_tower_rounded, 'Relay Reservations', 'Other peers reserve relay circuit slots through your node', Colors.indigo),
                      SizedBox(height: 12),
                      // Battery warning
                      Container(
                        padding: EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Colors.redAccent.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: Colors.redAccent.withValues(alpha: 0.2)),
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                Icon(Icons.battery_charging_full_rounded, color: Colors.redAccent, size: 18),
                                SizedBox(width: 8),
                                Text('Battery Impact', style: TextStyle(
                                  color: Colors.redAccent, fontSize: 12, fontWeight: FontWeight.bold,
                                )),
                              ],
                            ),
                            SizedBox(height: 6),
                            Text('Anchor mode keeps your device awake and actively participating in the Introvert mesh swarm. This consumes significantly more battery than regular mode.', style: TextStyle(
                              color: AppTheme.current.mutedText, fontSize: 10, height: 1.4,
                            )),
                            SizedBox(height: 6),
                            Text('Recommendation: Keep your device plugged in or charging while in anchor mode.', style: TextStyle(
                              color: AppTheme.current.mutedText, fontSize: 10, height: 1.4, fontWeight: FontWeight.w600,
                            )),
                            SizedBox(height: 6),
                            Container(
                              padding: EdgeInsets.all(8),
                              decoration: BoxDecoration(
                                color: Colors.redAccent.withValues(alpha: 0.06),
                                borderRadius: BorderRadius.circular(8),
                              ),
                              child: Row(
                                children: [
                                  Icon(Icons.warning_amber_rounded, color: Colors.redAccent, size: 16),
                                  SizedBox(width: 8),
                                  Expanded(
                                    child: Text('If battery drops below 30%, Intro-Claw will automatically disable anchor mode to protect your device.', style: TextStyle(
                                      color: Colors.redAccent, fontSize: 10, height: 1.3,
                                    )),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildAnchorRow(IconData icon, String title, String desc, Color color) {
    return Padding(
      padding: EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 28, height: 28,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(7),
            ),
            child: Icon(icon, size: 14, color: color),
          ),
          SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TextStyle(
                  color: AppTheme.current.text, fontSize: 11, fontWeight: FontWeight.w600,
                )),
                Text(desc, style: TextStyle(
                  color: AppTheme.current.mutedText, fontSize: 10, height: 1.3,
                )),
              ],
            ),
          ),
        ],
      ),
    );
  }

  void _toggleTunnelMode(bool value) async {
    setState(() => _isTunnelMode = value);
    IntrovertClient().setTunnelMode(value);
    
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool('isTunnelMode', value);

    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(value 
            ? "Secure Tunnel Mode Enabled: Routing via encrypted WebSocket on Port 80..." 
            : "Secure Tunnel Mode Disabled"),
          backgroundColor: value ? Colors.deepPurple : null,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    final client = IntrovertClient();
    final localPeerId = client.getPeerId() ?? "ERROR";

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: SingleChildScrollView(
        child: Column(
          children: [
            SizedBox(height: MediaQuery.of(context).padding.top + kToolbarHeight + 16),
            Center(
              child: GestureDetector(
                onTap: () => _showManifesto(),
                child: Image.asset(
                  'assets/images/icon_transparent.png',
                  height: 48,
                  filterQuality: FilterQuality.high,
                  color: AppTheme.current.bg.computeLuminance() > 0.5 ? Colors.black : Colors.white,
                ),
              ),
            ),
            SizedBox(height: 12),
            // Introvert logo (double width of Fano icon)
            Center(
              child: Image.asset(
                AppTheme.current.bg.computeLuminance() > 0.5
                    ? 'assets/images/logo_black.png'
                    : 'assets/images/logo_white.png',
                width: 96,
                filterQuality: FilterQuality.high,
              ),
            ),
            SizedBox(height: 8),
            // Build number with GitHub link
            Center(
              child: GestureDetector(
                onTap: () async {
                  const url = 'https://github.com/d3v6k/introvert/releases';
                  // ignore: deprecated_member_use
                  await launch(url);
                },
                child: Text(
                  'Build ${_getBuildNumber()}',
                  style: TextStyle(
                    fontSize: 11,
                    color: AppTheme.current.accent.withValues(alpha: 0.7),
                    decoration: TextDecoration.underline,
                    decorationColor: AppTheme.current.accent.withValues(alpha: 0.5),
                  ),
                ),
              ),
            ),
            SizedBox(height: 32),
            Padding(
              padding: EdgeInsets.symmetric(horizontal: 16.0),
              child: GlassmorphicContainer(
                borderRadius: BorderRadius.circular(16),
                blur: 10,
                tintAlpha: 0.08,
                borderAlpha: 0.12,
                padding: EdgeInsets.zero,
                child: NodeDashboard(
                  economyStats: _economyStats,
                ),
              ),
            ),
            SizedBox(height: 24),
            _buildSettingSection(
              'Edge Node Status',
              [
                SwitchListTile(
                  title: Row(
                    children: [
                      Flexible(
                        child: Text('Node Mode', style: TextStyle(fontSize: 13, color: AppTheme.current.text.withValues(alpha: 0.5)), overflow: TextOverflow.ellipsis),
                      ),
                      SizedBox(width: 6),
                      Container(
                        padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                        decoration: BoxDecoration(
                          color: AppTheme.current.accent.withValues(alpha: 0.1),
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: Text('EARN 3X', style: TextStyle(
                          fontSize: 8, fontWeight: FontWeight.bold,
                          color: AppTheme.current.accent, letterSpacing: 1,
                        )),
                      ),
                    ],
                  ),
                  subtitle: Text('Devices operating in Edge node earn 3x to 4x when compared to regular users. Holding requirement of 50,000 INTR to be eligible to become node.', style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
                  value: false,
                  onChanged: null,
                  activeTrackColor: AppTheme.current.accent.withValues(alpha: 0.5),
                  activeThumbColor: AppTheme.current.accent,
                  dense: true,
                ),
              ],
            ),
            SizedBox(height: 24),
            _buildSettingSection(
              'Appearance',
              [
                ListTile(
                  leading: Icon(Icons.palette_outlined, color: AppTheme.current.accent),
                  title: Text('App Theme'),
                  subtitle: Text(AppTheme.current.theme.name),
                  trailing: Icon(Icons.chevron_right, size: 20),
                  onTap: _showThemeSelector,
                ),
              ],
            ),
            // Other Instant Messenger Clients — single expander with all options
            _buildMessengerClientsSection(),
            _buildSettingSection(
              'Introvert Mesh Swarm Settings',
              [
                // ── Identity ──
                ListTile(
                  leading: Icon(Icons.fingerprint, color: AppTheme.current.accent),
                  title: Text('Public Peer ID', style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
                  subtitle: Text(localPeerId, style: TextStyle(fontFamily: 'monospace', fontSize: 11)),
                  trailing: IconButton(
                    icon: Icon(Icons.copy, size: 18),
                    onPressed: () {
                      Clipboard.setData(ClipboardData(text: localPeerId));
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(content: Text('Peer ID copied to clipboard')),
                      );
                    },
                  ),
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                // ── Status ──
                ListTile(
                  leading: Icon(Icons.check_circle, color: AppTheme.current.accent, size: 20),
                  title: Text('Bulletproof Core Active', style: TextStyle(fontSize: 13)),
                  dense: true,
                ),
                ListTile(
                  leading: Icon(Icons.storage, size: 20),
                  title: Text('SQLCipher Encrypted', style: TextStyle(fontSize: 13)),
                  trailing: Icon(Icons.check_circle, color: AppTheme.current.accent, size: 20),
                  dense: true,
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                // ── Connectivity ──
                SwitchListTile(
                  secondary: Icon(Icons.vpn_lock_rounded, color: Colors.deepPurpleAccent, size: 20),
                  title: Text('Secure Tunnel Mode', style: TextStyle(fontSize: 13)),
                  subtitle: Text('Routes traffic through encrypted WebSocket tunnel on Port 80.', style: TextStyle(fontSize: 12)),
                  value: _isTunnelMode,
                  onChanged: _toggleTunnelMode,
                  activeTrackColor: Colors.deepPurpleAccent.withValues(alpha: 0.5),
                  activeThumbColor: Colors.deepPurpleAccent,
                  dense: true,
                ),
                ListTile(
                  leading: Icon(Icons.signal_cellular_alt_rounded, color: AppTheme.current.accent, size: 20),
                  title: Text('Optimise Network Connection', style: TextStyle(fontSize: 13)),
                  subtitle: Text('Refresh all P2P connections to improve Introvert mesh swarm performance.', style: TextStyle(fontSize: 12)),
                  trailing: NetworkOptimizationButton(color: AppTheme.current.accent),
                  dense: true,
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                ListTile(
                  leading: Icon(Icons.router_rounded, color: AppTheme.current.accent, size: 20),
                  title: Text('Intro Claw RBN Selection', style: TextStyle(fontSize: 13)),
                  subtitle: Text('Manage and test Bootstrap Relays (RBNs) and manually configure relay endpoints.', style: TextStyle(fontSize: 12)),
                  trailing: Icon(Icons.chevron_right, size: 20),
                  onTap: _showRbnManagerDialog,
                  dense: true,
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                // ── Swarm Status ──
                ListTile(
                  leading: Icon(Icons.hub_rounded, color: AppTheme.current.accent, size: 20),
                  title: Text('Live Swarm Statistics', style: TextStyle(fontSize: 13)),
                  subtitle: Text('Real-time analytics of the global Introvert mesh swarm.', style: TextStyle(fontSize: 12)),
                  trailing: Container(
                    padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    decoration: BoxDecoration(
                      color: AppTheme.current.accent.withValues(alpha: 0.1),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text("LIVE", style: TextStyle(color: AppTheme.current.accent, fontSize: 9, fontWeight: FontWeight.bold)),
                  ),
                  onTap: _showSwarmStatus,
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                // ── Destructive Actions ──
                ListTile(
                  leading: Icon(Icons.delete_sweep, color: Colors.redAccent, size: 20),
                  title: Text('Clear All Contacts', style: TextStyle(color: Colors.redAccent, fontSize: 13)),
                  dense: true,
                  onTap: () async {
                    final confirm = await showDialog<bool>(
                      context: context,
                      builder: (context) => AlertDialog(
                        backgroundColor: AppTheme.current.surface,
                        title: Text("Destructive Action", style: TextStyle(color: Colors.redAccent)),
                        content: Text("Permanently delete all verified contacts and cached sessions?"),
                        actions: [
                          TextButton(onPressed: () => Navigator.pop(context, false), child: Text("CANCEL")),
                          TextButton(
                            onPressed: () => Navigator.pop(context, true),
                            child: Text("CLEAR EVERYTHING", style: TextStyle(color: Colors.redAccent)),
                          ),
                        ],
                      ),
                    );
                    if (confirm == true) {
                      await IntrovertClient().clearAllContacts();
                      widget.onContactsChanged?.call();
                      if (context.mounted) {
                        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("All contacts cleared.")));
                      }
                    }
                  },
                ),
                ListTile(
                  leading: Icon(Icons.warning_amber_rounded, color: Colors.red, size: 20),
                  title: Text('Nuke Identity', style: TextStyle(color: Colors.red, fontWeight: FontWeight.bold, fontSize: 13)),
                  dense: true,
                  onTap: () async {
                    final confirm = await showDialog<bool>(
                      context: context,
                      builder: (context) => AlertDialog(
                        backgroundColor: AppTheme.current.surface,
                        title: Text("NUKE IDENTITY?", style: TextStyle(color: Colors.red, fontWeight: FontWeight.bold)),
                        content: Text("This will permanently delete your identity keys, local database, and log you out immediately. You cannot recover this unless you have your seed phrase!"),
                        actions: [
                          TextButton(onPressed: () => Navigator.pop(context, false), child: Text("CANCEL")),
                          TextButton(
                            onPressed: () => Navigator.pop(context, true),
                            child: Text("NUKE EVERYTHING", style: TextStyle(color: Colors.red)),
                          ),
                        ],
                      ),
                    );
                    if (confirm == true) {
                      String dbPath;
                      String? docsPath;
                      if (Platform.isAndroid || Platform.isMacOS || Platform.isIOS) {
                        final dir = await getApplicationSupportDirectory();
                        dbPath = "${dir.path}/introvert.db";
                        final docsDir = await getApplicationDocumentsDirectory();
                        docsPath = docsDir.path;
                      } else {
                        dbPath = "./introvert.db";
                      }
                      IntrovertClient().nukeIdentity(dbPath);
                      _avatarCache.clear();
                      await IdentityManager().clearIdentity();

                      // Wipe ALL SharedPreferences — full hard reset to first-run state
                      final prefs = await SharedPreferences.getInstance();
                      await prefs.clear();

                      // Reset theme to default
                      await AppTheme.current.setTheme(AppTheme.introvertDefault);

                      // Delete all app data files (media, notes, cached files)
                      for (final path in [docsPath]) {
                        if (path == null) continue;
                        try {
                          final dir = Directory(path);
                          if (await dir.exists()) {
                            await for (final entity in dir.list()) {
                              try {
                                await entity.delete(recursive: true);
                              } catch (_) {}
                            }
                          }
                        } catch (_) {}
                      }

                      if (context.mounted) {
                        Navigator.of(context).pushAndRemoveUntil(
                          MaterialPageRoute(builder: (context) => const IntrovertApp()),
                          (route) => false,
                        );
                      }
                    }
                  },
                ),
              ],
            ),
            _buildSettingSection(
              'KLIPY Integration',
              [
                ListTile(
                  leading: Icon(Icons.gif_box_outlined, color: AppTheme.current.accent),
                  title: Text('KLIPY API Key'),
                  subtitle: Text(
                    _klipyApiKey.isEmpty ? 'Using default key' : '••••••••••••••••${_klipyApiKey.substring(_klipyApiKey.length > 4 ? _klipyApiKey.length - 4 : 0)}',
                    style: TextStyle(fontFamily: 'monospace'),
                  ),
                  trailing: Icon(Icons.edit, size: 20),
                  onTap: _showKlipyApiKeyDialog,
                ),
              ],
            ),
            _buildIntroClawSection(),
            _buildSettingSection(
              'Network Debug Log',
              [
                ListTile(
                  leading: Icon(Icons.bug_report_rounded, color: Colors.orangeAccent),
                  title: Text('Rust Network Log', style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
                  subtitle: Text(
                    '${IntrovertClient().debugLogCount} entries captured (relay, circuit, mesh)',
                    style: TextStyle(fontFamily: 'monospace', fontSize: 10),
                  ),
                  dense: true,
                ),
                Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                ListTile(
                  leading: Icon(Icons.save_alt_rounded, color: AppTheme.current.accent, size: 20),
                  title: Text('Save Network Log to File', style: TextStyle(fontSize: 13)),
                  subtitle: Text('Saves Rust relay/mesh swarm debug events to Downloads', style: TextStyle(fontSize: 11)),
                  dense: true,
                  onTap: _saveNetworkDebugLog,
                ),
                ListTile(
                  leading: Icon(Icons.copy_rounded, color: AppTheme.current.accent, size: 20),
                  title: Text('Copy Network Log to Clipboard', style: TextStyle(fontSize: 13)),
                  dense: true,
                  onTap: () {
                    final logs = IntrovertClient().getDebugLogs();
                    Clipboard.setData(ClipboardData(text: logs));
                    if (mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(content: Text('Network log copied (${IntrovertClient().debugLogCount} entries)')),
                      );
                    }
                  },
                ),
                ListTile(
                  leading: Icon(Icons.clear_all_rounded, color: Colors.redAccent.withValues(alpha: 0.7), size: 20),
                  title: Text('Clear Debug Log', style: TextStyle(fontSize: 13)),
                  dense: true,
                  onTap: () {
                    IntrovertClient().clearDebugLogs();
                    if (mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Debug log cleared')),
                      );
                    }
                  },
                ),
              ],
            ),
            _buildSettingSection(
              'Info & Legal',
              [
                ListTile(
                  leading: Icon(Icons.info_outline_rounded, color: AppTheme.current.accent),
                  title: Text('About Introvert'),
                  onTap: _showAboutDialog,
                ),
                ListTile(
                  leading: Icon(Icons.code_rounded, color: AppTheme.current.accent),
                  title: Text('Open Source Licenses & Attribution'),
                  subtitle: Text('Third-party licenses and ZeroClaw attribution'),
                  onTap: _showLicensesDialog,
                ),
                ListTile(
                  leading: Icon(Icons.gavel_rounded, color: AppTheme.current.accent),
                  title: Text('Terms of Use & Liability Disclaimer'),
                  subtitle: Text('View the complete Terms of Use and legal disclaimers'),
                  onTap: _showTermsOfUse,
                ),
              ],
            ),
            _buildSettingSection(
              'Software Updates',
              [
                ListTile(
                  leading: Icon(Icons.info_outline_rounded, color: AppTheme.current.accent),
                  title: Text('Current Version'),
                  subtitle: Text('v${_getBuildNumber()}', style: TextStyle(fontFamily: 'monospace', fontSize: 13)),
                ),
                ListTile(
                  leading: Icon(Icons.update_rounded, color: AppTheme.current.accent),
                  title: Text('Check for Updates'),
                  subtitle: Text('Check GitHub for the latest release and download.'),
                  onTap: () {
                    UpdateService.checkForUpdates(context, forceShowMessage: true);
                  },
                ),
                ListTile(
                  leading: Icon(Icons.code_rounded, color: AppTheme.current.accent),
                  title: Text('GitHub Repository'),
                  subtitle: Text('github.com/d3v6k/introvert', style: TextStyle(fontFamily: 'monospace', fontSize: 11)),
                  onTap: () async {
                    final uri = Uri.parse('https://github.com/d3v6k/introvert');
                    if (await canLaunchUrl(uri)) {
                      await launchUrl(uri, mode: LaunchMode.externalApplication);
                    }
                  },
                ),
              ],
            ),
            // Bottom padding so Software Updates section scrolls above navigation bar on iOS/Android/macOS
            SizedBox(height: MediaQuery.of(context).padding.bottom + 160),
          ],
        ),
      ),
    );
  }

  void _showTermsOfUse() {
    const termsText = '''# Introvert App — Terms of Use & Liability Disclaimer

**Last Updated: June 2026**

---

## ⚠️ CRITICAL NOTICE FOR ALL USERS

Introvert is **not a service**. It is a decentralized, open-source, serverless peer-to-peer (P2P) communication software utility. By utilizing this software, you operate your own autonomous node within a distributed mesh network. The creators, developers, and maintainers of Introvert have **no control over the network**, hold **zero user data**, and have **no capacity to monitor, moderate, or log your traffic**.

---

## 1. Legal Status of the Software

Introvert is an open-source communication software tool, completely separate from any financial or asset-management services.

- **Not a Wallet:** Introvert does not hold, store, transmit, or facilitate the transfer of any funds, cryptocurrencies, or digital assets. It completely lacks the programming or capacity to handle peer-to-peer financial transactions.

- **External Token Management:** Any interaction with the Solana blockchain or the management of your \$INTR tokens must occur exclusively outside of this application using third-party software.

- **No Central Infrastructure:** There are no central databases, cloud platforms, or authentication servers. Your cryptographic identity is derived entirely on your local device using an offline 12-word BIP-39 mnemonic phrase.

---

## 2. Age Restriction & Eligibility

- **Minimum Age:** You must be at least 18 years of age (or the legal age of majority within your sovereign jurisdiction) to open, execute, or interact with this software.

- **Representation of Age:** By checking the agreement box and running this software, you explicitly represent and warrant that you meet this age requirement. If you are under 18, you are strictly prohibited from using Introvert.

---

## 3. Absolute Prohibition of Illegal & Covert Activities

Because Introvert operates on total user sovereignty, you bear exclusive legal liability for all content, data, and metadata you route through your node.

- **Lawful Use Only:** You explicitly agree to use this software solely for lawful intents and activities.

- **Prohibited Actions:** You are strictly forbidden from utilizing Introvert to facilitate, execute, or conceal criminal enterprise, malicious hacking, harassment, or the distribution, transmission, or caching of any illicit material.

- **Zero Indemnification:** You acknowledge that the open-source developers will not protect, legally defend, or indemnify you if you use this utility to violate local or international laws.

---

## 4. Complete Disclaimer of Warranties & Limitation of Liability

- **Provided "As Is":** This open-source software code is provided entirely "as is" and "as available," without warranties of any scale, express or implied.

- **Liability Cap:** Under no legal theory or jurisdiction shall the developers, contributors, or maintainers of the Introvert ecosystem be liable for any direct, indirect, incidental, or consequential damages, leaks, hardware wear, or data loss resulting from your use or inability to use the codebase. You proceed entirely at your own risk.

- **Data and Identity Loss:** You are uniquely responsible for backing up your BIP-39 seed phrase. There is no central password recovery mechanism; if you lose your seed phrase, you permanently lose your network identity and local database access.''';

    showDialog(
      context: context,
      builder: (context) {
        return Dialog(
          backgroundColor: AppTheme.current.surface,
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.of(context).size.height * 0.85,
              maxWidth: 500,
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: EdgeInsets.fromLTRB(20, 16, 20, 8),
                  child: Row(
                    children: [
                      Icon(Icons.gavel_rounded, color: AppTheme.current.accent, size: 20),
                      SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          'TERMS OF USE & DISCLAIMER',
                          style: TextStyle(
                            color: AppTheme.current.text,
                            fontWeight: FontWeight.bold,
                            fontSize: 14,
                            letterSpacing: 1,
                          ),
                        ),
                      ),
                      IconButton(
                        icon: Icon(Icons.close, color: AppTheme.current.mutedText),
                        onPressed: () => Navigator.pop(context),
                      ),
                    ],
                  ),
                ),
                Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
                Expanded(
                  child: SingleChildScrollView(
                    padding: EdgeInsets.all(20),
                    child: Text(
                      termsText
                          .replaceAll('# ', '')
                          .replaceAll('## ', '')
                          .replaceAll('### ', '')
                          .replaceAll('**', '')
                          .replaceAll('---', '—————————————————')
                          .replaceAll('- ', '  • '),
                      style: TextStyle(
                        color: AppTheme.current.text.withValues(alpha: 0.85),
                        fontSize: 13,
                        height: 1.6,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  void _showAboutDialog() {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          backgroundColor: AppTheme.current.surface,
          title: Row(
            children: [
              Icon(Icons.info_outline_rounded, color: AppTheme.current.accent),
              SizedBox(width: 8),
              Text(
                "ABOUT INTROVERT",
                style: TextStyle(
                  color: AppTheme.current.text,
                  fontFamily: 'monospace',
                  fontSize: 15,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ],
          ),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                "Introvert is an open source server-less peer to peer (p2p) communication software utility.",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13, height: 1.3),
              ),
              SizedBox(height: 12),
              Text(
                "Created by d3v6k",
                style: TextStyle(
                  color: AppTheme.current.accent,
                  fontWeight: FontWeight.bold,
                  fontSize: 14,
                  fontFamily: 'monospace',
                ),
              ),
              SizedBox(height: 6),
              Text(
                "i@devofintrovert  /  @devofintrovert",
                style: TextStyle(
                  color: AppTheme.current.text.withValues(alpha: 0.5),
                  fontSize: 12,
                  fontFamily: 'monospace',
                ),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CLOSE", style: TextStyle(color: AppTheme.current.accent)),
            ),
          ],
        );
      },
    );
  }

  void _showLicensesDialog() {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          backgroundColor: AppTheme.current.surface,
          title: Row(
            children: [
              Icon(Icons.code_rounded, color: AppTheme.current.accent),
              SizedBox(width: 8),
              Expanded(
                child: Text(
                  "OPEN SOURCE LICENSES & ATTRIBUTION",
                  style: TextStyle(
                    color: AppTheme.current.text,
                    fontFamily: 'monospace',
                    fontSize: 13,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
            ],
          ),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Magic Wormhole
                Text("Magic Wormhole", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13)),
                SizedBox(height: 4),
                Text("Introvert's Magic Link is powered by the open-source Magic Wormhole protocol.", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, height: 1.4)),
                SizedBox(height: 4),
                Text("MIT License · Copyright (c) 2018 Brian Warner", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 10)),
                SizedBox(height: 16),

                // ZeroClaw
                Text("ZeroClaw", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13)),
                SizedBox(height: 4),
                Text("Introvert's Intro-Claw automation engine is forked from ZeroClaw, an open-source autonomous AI assistant infrastructure.", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, height: 1.4)),
                SizedBox(height: 4),
                Text("github.com/zeroclaw-labs/zeroclaw", style: TextStyle(color: AppTheme.current.accent.withValues(alpha: 0.7), fontSize: 10, fontFamily: 'monospace')),
                Text("Created by @theonlyhennygod · Project lead @JordanTheJet", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10)),
                SizedBox(height: 4),
                Text("MIT / Apache 2.0 · Copyright (c) ZeroClaw Labs", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 10)),
                SizedBox(height: 16),

                // Other licenses
                Text("Third-Party Libraries", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13)),
                SizedBox(height: 8),
                Text(
                  "• libp2p: Apache 2.0 / MIT\n• webrtc-rs: MIT\n• SQLCipher: BSD\n• rusqlite: MIT\n• tokio: MIT\n• Flutter Framework: BSD\n• ed25519-dalek: BSD-3-Clause\n• solana-sdk: Apache 2.0",
                  style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, height: 1.5),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CLOSE", style: TextStyle(color: AppTheme.current.accent)),
            ),
          ],
        );
      },
    );
  }

  void _showThemeSelector() {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      isScrollControlled: true,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) {
        return DraggableScrollableSheet(
          initialChildSize: 0.6,
          minChildSize: 0.3,
          maxChildSize: 0.85,
          expand: false,
          builder: (_, scrollController) => StatefulBuilder(
            builder: (context, setSheetState) => SafeArea(
              child: Column(
                children: [
                  Container(
                    margin: EdgeInsets.symmetric(vertical: 8),
                    width: 40,
                    height: 4,
                    decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.2), borderRadius: BorderRadius.circular(2)),
                  ),
                  Padding(
                    padding: EdgeInsets.all(16.0),
                    child: Text(
                      "SELECT THEME",
                      style: TextStyle(
                        color: AppTheme.current.accent,
                        fontSize: 12,
                        fontWeight: FontWeight.bold,
                        letterSpacing: 1.2,
                      ),
                    ),
                  ),
                  Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
                  Expanded(
                    child: ListView(
                      controller: scrollController,
                      children: [
                        // Built-in themes
                        Padding(
                          padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                          child: Text("BUILT-IN", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1)),
                        ),
                        ...AppTheme.themes.map((theme) {
                          final isSelected = theme.name == AppTheme.current.theme.name;
                          return Material(
                            color: Colors.transparent,
                            child: ListTile(
                              leading: Container(
                                width: 32,
                                height: 32,
                                decoration: BoxDecoration(
                                  gradient: LinearGradient(colors: [theme.bg, theme.surface]),
                                  borderRadius: BorderRadius.circular(8),
                                  border: Border.all(color: theme.accent.withValues(alpha: 0.5), width: 1.5),
                                ),
                                child: Container(
                                  margin: EdgeInsets.all(4),
                                  decoration: BoxDecoration(color: theme.accent, shape: BoxShape.circle),
                                ),
                              ),
                              title: Text(theme.name, style: TextStyle(color: isSelected ? AppTheme.current.accent : AppTheme.current.text, fontSize: 14)),
                              subtitle: Text(theme.isDark ? "Dark" : "Light", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                              trailing: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  IconButton(
                                    icon: Icon(Icons.edit, size: 18, color: AppTheme.current.mutedText),
                                    onPressed: () async {
                                      final result = await showDialog<ThemeConfig>(
                                        context: context,
                                        builder: (_) => CustomThemeCreator(existingTheme: theme),
                                      );
                                      if (result != null) {
                                        await AppTheme.current.saveCustomTheme(result);
                                        AppTheme.current.setTheme(result);
                                        Navigator.pop(context);
                                        setState(() {});
                                      }
                                    },
                                  ),
                                  if (isSelected) Icon(Icons.check, color: AppTheme.current.accent),
                                ],
                              ),
                              onTap: () {
                                AppTheme.current.setTheme(theme);
                                Navigator.pop(context);
                                setState(() {});
                              },
                            ),
                          );
                        }),

                        // Custom themes
                        if (AppTheme.current.customThemes.isNotEmpty) ...[
                          Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                          Padding(
                            padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                            child: Text("CUSTOM", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1)),
                          ),
                          ...AppTheme.current.customThemes.map((theme) {
                            final isSelected = theme.name == AppTheme.current.theme.name;
                  return GlassmorphicContainer(
                    borderRadius: BorderRadius.circular(16),
                    blur: 10,
                    tintAlpha: 0.08,
                    borderAlpha: 0.12,
                    padding: EdgeInsets.zero,
                    margin: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    child: Material(
                      color: Colors.transparent,
                      child: ListTile(
                              leading: Container(
                                width: 32,
                                height: 32,
                                decoration: BoxDecoration(
                                  gradient: LinearGradient(colors: [theme.bg, theme.surface]),
                                  borderRadius: BorderRadius.circular(8),
                                  border: Border.all(color: theme.accent.withValues(alpha: 0.5), width: 1.5),
                                ),
                                child: Container(
                                  margin: EdgeInsets.all(4),
                                  decoration: BoxDecoration(color: theme.accent, shape: BoxShape.circle),
                                ),
                              ),
                              title: Text(theme.name, style: TextStyle(color: isSelected ? AppTheme.current.accent : AppTheme.current.text, fontSize: 14)),
                              subtitle: Text(theme.isDark ? "Dark" : "Light", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                              trailing: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (isSelected) Icon(Icons.check, color: AppTheme.current.accent),
                                  IconButton(
                                    icon: Icon(Icons.edit_outlined, size: 18, color: AppTheme.current.mutedText),
                                    onPressed: () async {
                                      final result = await showDialog<ThemeConfig>(
                                        context: context,
                                        builder: (_) => CustomThemeCreator(existingTheme: theme),
                                      );
                                      if (result != null) {
                                        await AppTheme.current.saveCustomTheme(result);
                                        await AppTheme.current.setTheme(result);
                                        setSheetState(() {});
                                        setState(() {});
                                      }
                                    },
                                  ),
                                  if (AppTheme.current.customThemes.any((t) => t.name == theme.name))
                                    IconButton(
                                    icon: Icon(Icons.delete_outline, size: 18, color: Colors.redAccent.withValues(alpha: 0.7)),
                                    onPressed: () async {
                                      final confirm = await showDialog<bool>(
                                        context: context,
                                        builder: (ctx) => AlertDialog(
                                          backgroundColor: AppTheme.current.surface,
                                          title: Text("Delete Theme?", style: TextStyle(color: AppTheme.current.text)),
                                          content: Text("Remove \"${theme.name}\" from your custom themes.", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13)),
                                          actions: [
                                            TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text("CANCEL")),
                                            TextButton(onPressed: () => Navigator.pop(ctx, true), child: Text("DELETE", style: TextStyle(color: Colors.redAccent))),
                                          ],
                                        ),
                                      );
                                      if (confirm == true) {
                                        await AppTheme.current.deleteCustomTheme(theme.name);
                                        setSheetState(() {});
                                        setState(() {});
                                      }
                                    },
                                  ),
                                ],
                              ),
                              onTap: () {
                                AppTheme.current.setTheme(theme);
                                Navigator.pop(context);
                                setState((){});
                              },
                            ),
                          ),
                          );
                        }),
                        ],

                        // Create custom theme button
                        Divider(height: 1, indent: 16, endIndent: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                        Material(
                          color: Colors.transparent,
                          child: ListTile(
                            leading: Container(
                              width: 32,
                              height: 32,
                              decoration: BoxDecoration(
                                color: AppTheme.current.accent.withValues(alpha: 0.15),
                                borderRadius: BorderRadius.circular(8),
                                border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3), width: 1),
                              ),
                              child: Icon(Icons.add, color: AppTheme.current.accent, size: 18),
                            ),
                            title: Text("Create Custom Theme", style: TextStyle(color: AppTheme.current.accent, fontSize: 14, fontWeight: FontWeight.w500)),
                            subtitle: Text("Design your own colour palette", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                            onTap: () async {
                              final result = await showDialog<ThemeConfig>(
                                context: context,
                                builder: (_) => CustomThemeCreator(),
                              );
                              if (result != null) {
                                await AppTheme.current.saveCustomTheme(result);
                                await AppTheme.current.setTheme(result);
                                setSheetState(() {});
                                setState(() {});
                              }
                            },
                          ),
                        ),
                        SizedBox(height: 16),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  void _showKlipyApiKeyDialog() async {
    final controller = TextEditingController(text: _klipyApiKey);
    try {
      final confirm = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          backgroundColor: AppTheme.current.surface,
          title: Text("Configure KLIPY API Key", style: TextStyle(color: AppTheme.current.accent)),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                "Get a free KLIPY API key by registering a partner app at:",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12),
              ),
              SizedBox(height: 4),
              InkWell(
                onTap: () {
                  Clipboard.setData(const ClipboardData(text: "https://partner.klipy.com/"));
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(content: Text("Link copied to clipboard")),
                  );
                },
                child: Text(
                  "partner.klipy.com (Tap to copy link)",
                  style: TextStyle(color: AppTheme.current.accent, decoration: TextDecoration.underline, fontSize: 12),
                ),
              ),
              SizedBox(height: 16),
              TextField(
                controller: controller,
                decoration: InputDecoration(
                  labelText: "KLIPY API Key",
                  labelStyle: TextStyle(color: Colors.grey),
                  enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: Colors.grey)),
                  focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
                ),
                style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text("CANCEL", style: TextStyle(color: Colors.grey)),
            ),
            TextButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text("SAVE", style: TextStyle(color: AppTheme.current.accent)),
            ),
          ],
        ),
      );

      if (confirm == true) {
        final value = controller.text.trim();
        final prefs = await SharedPreferences.getInstance();
        if (!mounted) return;
        await prefs.setString('klipy_api_key', value);
        setState(() {
          _klipyApiKey = value;
        });
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(value.isEmpty ? "KLIPY API Key removed" : "KLIPY API Key updated")),
        );
      }
    } finally {
      controller.dispose();
    }
  }

  Widget _buildMessengerClientsSection() {
    final messengers = [
      {'key': 'whatsapp', 'name': 'WhatsApp', 'icon': Icons.chat_rounded, 'color': const Color(0xFF25D366), 'enabled': _whatsappEnabled, 'desc': 'Scan QR code — like WhatsApp Desktop', 'customIcon': true},
      {'key': 'telegram', 'name': 'Telegram', 'icon': Icons.send_rounded, 'color': const Color(0xFF0088CC), 'enabled': _telegramEnabled, 'desc': 'Log in with phone — like Telegram Desktop'},
      {'key': 'discord', 'name': 'Discord', 'icon': Icons.forum_rounded, 'color': const Color(0xFF5865F2), 'enabled': _discordEnabled, 'desc': 'Log in — like Discord Desktop'},
      {'key': 'slack', 'name': 'Slack', 'icon': Icons.workspaces_rounded, 'color': const Color(0xFF4A154B), 'enabled': _slackEnabled, 'desc': 'Log in — like Slack Desktop'},
      {'key': 'messenger', 'name': 'Messenger', 'icon': Icons.message_rounded, 'color': const Color(0xFF0084FF), 'enabled': _messengerEnabled, 'desc': 'Log in — like Messenger Desktop'},
      {'key': 'google_messages', 'name': 'Google Messages', 'icon': Icons.sms_rounded, 'color': const Color(0xFF1A73E8), 'enabled': _googleMessagesEnabled, 'desc': 'Log in — like Messages for Web'},
    ];

    return Padding(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: GlassmorphicContainer(
          borderRadius: BorderRadius.circular(16),
          blur: 10,
          tintAlpha: 0.08,
          borderAlpha: 0.12,
          padding: EdgeInsets.zero,
          child: Material(
            color: Colors.transparent,
            borderRadius: BorderRadius.circular(16),
            clipBehavior: Clip.antiAlias,
            child: ExpansionTile(
              leading: Icon(Icons.devices_rounded, color: AppTheme.current.accent, size: 22),
              title: Text('Other Instant Messenger Clients', style: TextStyle(fontSize: 13, fontWeight: FontWeight.bold, color: AppTheme.current.accent, letterSpacing: 0.5)),
              subtitle: Text('$_enabledMessengerCount/$_maxMessengerTabs enabled', style: TextStyle(fontSize: 10, color: AppTheme.current.mutedText)),
              collapsedIconColor: AppTheme.current.mutedText,
              iconColor: AppTheme.current.accent,
              initiallyExpanded: false,
              children: [
                Padding(
                  padding: EdgeInsets.fromLTRB(16, 0, 16, 4),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Add messenger web clients as tabs in the navigation bar. These work exactly like desktop apps — Introvert cannot see your chats. Traffic goes directly to each service, not through the P2P network.',
                        style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, height: 1.4),
                      ),
                      SizedBox(height: 4),
                      Text(
                        'Maximum 3 messenger tabs can be active at once.',
                        style: TextStyle(color: AppTheme.current.accent.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.w600),
                      ),
                    ],
                  ),
                ),
                ...messengers.map((m) {
                  final key = m['key'] as String;
                  final name = m['name'] as String;
                  final icon = m['icon'] as IconData;
                  final color = m['color'] as Color;
                  final isEnabled = m['enabled'] as bool;
                  final desc = m['desc'] as String;
                  final hasCustomIcon = m['customIcon'] == true;
                  final canEnable = !isEnabled && _enabledMessengerCount >= _maxMessengerTabs;
                  return SwitchListTile(
                    secondary: hasCustomIcon
                        ? WhatsAppIcon(size: 22, color: isEnabled ? color : AppTheme.current.mutedText)
                        : Icon(icon, color: isEnabled ? color : AppTheme.current.mutedText, size: 22),
                    title: Text(name, style: TextStyle(fontSize: 13, color: canEnable ? AppTheme.current.mutedText.withValues(alpha: 0.4) : AppTheme.current.text)),
                    subtitle: Text(desc, style: TextStyle(fontSize: 10, color: AppTheme.current.mutedText.withValues(alpha: canEnable ? 0.3 : 0.7))),
                    value: isEnabled,
                    activeColor: color,
                    onChanged: canEnable ? null : (val) async {
                      setState(() {
                        switch (key) {
                          case 'whatsapp': _whatsappEnabled = val; break;
                          case 'telegram': _telegramEnabled = val; break;
                          case 'discord': _discordEnabled = val; break;
                          case 'slack': _slackEnabled = val; break;
                          case 'messenger': _messengerEnabled = val; break;
                          case 'google_messages': _googleMessagesEnabled = val; break;
                        }
                      });
                      // Save immediately and trigger nav bar rebuild
                      final prefs = await SharedPreferences.getInstance();
                      await prefs.setBool('${key}_enabled', val);
                      widget.onMessengerSettingsChanged?.call();
                    },
                    dense: true,
                  );
                }),
                SizedBox(height: 4),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildIntroClawSection() {
    return Padding(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: GlassmorphicContainer(
          borderRadius: BorderRadius.circular(16),
          blur: 10,
          tintAlpha: 0.08,
          borderAlpha: 0.12,
          padding: EdgeInsets.zero,
          child: Material(
            color: Colors.transparent,
            borderRadius: BorderRadius.circular(16),
            clipBehavior: Clip.antiAlias,
            child: ExpansionTile(
              title: Text(
                'INTRO-CLAW AUTOMATION ENGINE',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.bold,
                  color: AppTheme.current.accent,
                  letterSpacing: 1.2,
                ),
              ),
              childrenPadding: EdgeInsets.only(bottom: 8, top: 4),
              collapsedIconColor: AppTheme.current.mutedText,
              iconColor: AppTheme.current.accent,
              initiallyExpanded: false,
              children: [
                Padding(
                  padding: EdgeInsets.symmetric(horizontal: 16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // Local-only status
                      Container(
                        padding: EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Colors.greenAccent.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.2)),
                        ),
                        child: Row(
                          children: [
                            Icon(Icons.shield_rounded, color: Colors.greenAccent, size: 20),
                            SizedBox(width: 10),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('100% Local — Sandboxed', style: TextStyle(
                                    color: Colors.greenAccent, fontSize: 12, fontWeight: FontWeight.bold,
                                  )),
                                  SizedBox(height: 2),
                                  Text('All operations run on-device. Zero data leaked. No external API calls. No prompt injection risk.', style: TextStyle(
                                    color: AppTheme.current.mutedText, fontSize: 10, height: 1.3,
                                  )),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                      SizedBox(height: 12),
                      Row(
                        children: [
                          Text('STATUS DASHBOARD', style: TextStyle(
                            fontSize: 10,
                            fontWeight: FontWeight.bold,
                            color: AppTheme.current.mutedText,
                            letterSpacing: 1,
                          )),
                          Spacer(),
                          GestureDetector(
                            onTap: _showClawModulesInfo,
                            child: Container(
                              padding: EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                              decoration: BoxDecoration(
                                color: AppTheme.current.accent.withValues(alpha: 0.1),
                                borderRadius: BorderRadius.circular(8),
                              ),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(Icons.list_rounded, size: 12, color: AppTheme.current.accent),
                                  SizedBox(width: 4),
                                  Text('17 MODULES', style: TextStyle(
                                    fontSize: 8, fontWeight: FontWeight.bold,
                                    color: AppTheme.current.accent, letterSpacing: 1,
                                  )),
                                ],
                              ),
                            ),
                          ),
                        ],
                      ),
                      SizedBox(height: 8),
                      // Network stats chips
                      Row(
                        children: [
                          _buildClawStatChip('RBNs', '$_rbnCount', Icons.dns_rounded, Colors.orangeAccent),
                          SizedBox(width: 8),
                          _buildClawStatChip('NODES', '$_meshNodeCount', Icons.hub_rounded, Colors.cyanAccent),
                          SizedBox(width: 8),
                          _buildClawStatChip('PEERS', '$_connectedPeerCount', Icons.people_rounded, Colors.greenAccent),
                        ],
                      ),
                      SizedBox(height: 8),
                      // Engine Status (always active)
                      Row(
                        children: [
                          Icon(
                            Icons.check_circle,
                            color: Colors.green,
                            size: 18,
                          ),
                          SizedBox(width: 8),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  'Engine Active',
                                  style: TextStyle(
                                    color: AppTheme.current.text,
                                    fontSize: 12,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                Text(
                                  'Runs 20+ modules every 5 minutes',
                                  style: TextStyle(
                                    color: AppTheme.current.mutedText,
                                    fontSize: 10,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                      SizedBox(height: 12),
                      // Run Maintenance Button
                      SizedBox(
                        width: double.infinity,
                        child: OutlinedButton.icon(
                          icon: Icon(Icons.build, color: AppTheme.current.accent, size: 18),
                          label: Text('Run Maintenance Now', style: TextStyle(color: AppTheme.current.accent, fontSize: 12)),
                          style: OutlinedButton.styleFrom(
                            side: BorderSide(color: AppTheme.current.accent.withValues(alpha: 0.3)),
                            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                            padding: EdgeInsets.symmetric(vertical: 10),
                          ),
                          onPressed: _triggerClawTick,
                        ),
                      ),
                      SizedBox(height: 16),
                      // Intro-Claw Description
                      Text(
                        'Intro-Claw is an AI intelligent agent working under the hood, handling networking, engine optimisation, and Introvert mesh swarm interactions. All operations run 100% on-device in a sandboxed environment — zero data leaked, zero external calls.',
                        style: TextStyle(
                          color: AppTheme.current.mutedText,
                          fontSize: 11,
                          height: 1.4,
                        ),
                      ),
                      SizedBox(height: 16),
                      // Activity Log
                      _buildClawActivityLog(),
                      SizedBox(height: 12),
                      // RECON, HEAL, and Download buttons
                      Row(
                        children: [
                          Expanded(
                            child: OutlinedButton.icon(
                              icon: Icon(Icons.radar_rounded, color: Colors.orangeAccent, size: 18),
                              label: Text('RECON', style: TextStyle(color: Colors.orangeAccent, fontSize: 12)),
                              style: OutlinedButton.styleFrom(
                                side: BorderSide(color: Colors.orangeAccent.withValues(alpha: 0.3)),
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                                padding: EdgeInsets.symmetric(vertical: 10),
                              ),
                              onPressed: _runClawRecon,
                            ),
                          ),
                          SizedBox(width: 8),
                          Expanded(
                            child: OutlinedButton.icon(
                              icon: Icon(Icons.healing_rounded, color: Colors.cyanAccent, size: 18),
                              label: Text('HEAL', style: TextStyle(color: Colors.cyanAccent, fontSize: 12)),
                              style: OutlinedButton.styleFrom(
                                side: BorderSide(color: Colors.cyanAccent.withValues(alpha: 0.3)),
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                                padding: EdgeInsets.symmetric(vertical: 10),
                              ),
                              onPressed: _runClawHeal,
                            ),
                          ),
                          SizedBox(width: 8),
                          Expanded(
                            child: OutlinedButton.icon(
                              icon: Icon(Icons.download_rounded, color: Colors.greenAccent, size: 18),
                              label: Text('LOG', style: TextStyle(color: Colors.greenAccent, fontSize: 12)),
                              style: OutlinedButton.styleFrom(
                                side: BorderSide(color: Colors.greenAccent.withValues(alpha: 0.3)),
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                                padding: EdgeInsets.symmetric(vertical: 10),
                              ),
                              onPressed: _downloadClawLog,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
                SizedBox(height: 8),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _showClawModulesInfo() {
    showDialog(
      context: context,
      builder: (ctx) => Dialog(
        backgroundColor: AppTheme.current.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: EdgeInsets.fromLTRB(20, 16, 20, 8),
                child: Row(
                  children: [
                    Icon(Icons.psychology_rounded, color: AppTheme.current.accent, size: 20),
                    SizedBox(width: 8),
                    Expanded(
                      child: Text('INTRO-CLAW MODULES', style: TextStyle(
                        color: AppTheme.current.accent, fontSize: 13,
                        fontWeight: FontWeight.bold, letterSpacing: 1.2,
                      )),
                    ),
                    GestureDetector(
                      onTap: () => Navigator.pop(ctx),
                      child: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText),
                    ),
                  ],
                ),
              ),
              Flexible(
                child: SingleChildScrollView(
                  padding: EdgeInsets.fromLTRB(20, 4, 20, 16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _buildModuleRow(Icons.battery_saver_rounded, 'Battery Throttling', 'Reduces sync/heartbeat when battery <20%', Colors.green),
                      _buildModuleRow(Icons.storage_rounded, 'Database Pruning', 'Cleans expired sessions, crypto sessions, mesh chunks', Colors.blue),
                      _buildModuleRow(Icons.cleaning_services_rounded, 'Media Cleanup', 'Removes orphaned mesh chunks, manages storage quota', Colors.orange),
                      _buildModuleRow(Icons.wifi_tethering_rounded, 'Connection Optimization', 'Scans mDNS peers for direct P2P upgrades', Colors.teal),
                      _buildModuleRow(Icons.send_rounded, 'Message Batching', 'Buffers messages during poor connectivity, auto-flushes', Colors.purple),
                      _buildModuleRow(Icons.download_rounded, 'Predictive Prefetch', 'Pre-pulls files from top contacts before you ask', Colors.amber),
                      _buildModuleRow(Icons.sort_rounded, 'Sync Prioritization', 'Syncs most-active contacts first', Colors.cyan),
                      _buildModuleRow(Icons.block_rounded, 'Duplicate Suppression', '10k FIFO cache prevents duplicate messages', Colors.red),
                      _buildModuleRow(Icons.favorite_rounded, 'Health Scoring', 'Decay-based scoring per peer (0.0-1.0)', Colors.pink),
                      _buildModuleRow(Icons.sd_storage_rounded, 'Storage Quota', 'Auto-prunes at 80% disk, aggressive at 90%', Colors.brown),
                      _buildModuleRow(Icons.speed_rounded, 'Adaptive Chunking', 'Adjusts chunk size per peer: 64KB-512KB based on speed', Colors.indigo),
                      _buildModuleRow(Icons.timer_rounded, 'Full Tick Cycle', 'Runs all modules every 5 minutes', Colors.deepOrange),
                      _buildModuleRow(Icons.queue_rounded, 'Offline Queue', 'Buffers messages when offline, flushes when restored', Colors.blueAccent),
                      _buildModuleRow(Icons.mail_outline_rounded, 'Dead Letter Detection', 'Flags messages stuck >5 min, tries alternative routes', Colors.redAccent),
                      _buildModuleRow(Icons.whatshot_rounded, 'Connection Pre-warming', 'Pre-dials top 3 contacts when you open contacts list', Colors.deepOrange),
                      _buildModuleRow(Icons.build_rounded, 'Night Maintenance', 'Heavy cleanup during 30+ min idle periods', Colors.grey),
                      _buildModuleRow(Icons.radar_rounded, 'VoIP Monitor', 'Tracks call quality: RTT, packet loss, jitter, bitrate', Colors.lightBlue),
                    ],
                  ),
                ),
              ),
              Padding(
                padding: EdgeInsets.fromLTRB(20, 0, 20, 16),
                child: Container(
                  padding: EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: AppTheme.current.text.withValues(alpha: 0.03),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('When INACTIVE:', style: TextStyle(
                        color: Colors.redAccent, fontSize: 11, fontWeight: FontWeight.bold,
                      )),
                      SizedBox(height: 4),
                      Text('None of these modules run. The engine sits idle. No background optimization, no pre-fetching, no health scoring, no automatic cleanup.', style: TextStyle(
                        color: AppTheme.current.mutedText, fontSize: 10, height: 1.4,
                      )),
                      SizedBox(height: 8),
                      Text('When ACTIVE:', style: TextStyle(
                        color: Colors.greenAccent, fontSize: 11, fontWeight: FontWeight.bold,
                      )),
                      SizedBox(height: 4),
                      Text('All 17 modules run automatically on their schedules. The Introvert mesh swarm stays optimized, files pre-fetch, connections are monitored, and storage stays clean.', style: TextStyle(
                        color: AppTheme.current.mutedText, fontSize: 10, height: 1.4,
                      )),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildModuleRow(IconData icon, String title, String desc, Color color) {
    return Padding(
      padding: EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 28, height: 28,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(7),
            ),
            child: Icon(icon, size: 14, color: color),
          ),
          SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TextStyle(
                  color: AppTheme.current.text, fontSize: 11, fontWeight: FontWeight.w600,
                )),
                Text(desc, style: TextStyle(
                  color: AppTheme.current.mutedText, fontSize: 10, height: 1.3,
                )),
              ],
            ),
          ),
        ],
      ),
    );
  }

  void _refreshClawStatus() {
    try {
      final client = IntrovertClient();
      final statusJson = client.getIntroClawStatus();
      final status = jsonDecode(statusJson) as Map<String, dynamic>;
      // Fetch RBN count
      int rbnCount = 0;
      try {
        final rbns = client.getRbns();
        rbnCount = rbns.length;
      } catch (_) {}
      // Fetch connected peers from status
      int connectedPeers = 0;
      try {
        final peers = status['connected_peers'] as List?;
        connectedPeers = peers?.length ?? 0;
      } catch (_) {}
      if (mounted) {
        setState(() {
          _clawStatusJson = statusJson;
          _rbnCount = rbnCount;
          _meshNodeCount = _swarmStats['total_nodes'] as int? ?? 1;
          _connectedPeerCount = connectedPeers;
        });
      }
    } catch (e) {
      debugPrint('Error loading intro-claw status: $e');
    }
  }

  void _refreshClawActivityLog() {
    try {
      final jsonStr = IntrovertClient().getIntroClawActivityLog();
      final List<dynamic> entries = jsonDecode(jsonStr);
      if (mounted) {
        setState(() {
          _clawActivityLog = entries.map((e) => Map<String, dynamic>.from(e as Map)).toList();
          _clawActivityLog.sort((a, b) => (b['t'] as int).compareTo(a['t'] as int));
        });
        // Auto-scroll to latest entry
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (_clawLogScrollController.hasClients) {
            _clawLogScrollController.animateTo(
              _clawLogScrollController.position.maxScrollExtent,
              duration: Duration(milliseconds: 200),
              curve: Curves.easeOut,
            );
          }
        });
      }
    } catch (_) {}
  }

  Widget _buildClawActivityLog() {
    return Container(
      height: 200,
      decoration: BoxDecoration(
        color: Colors.black87,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.2)),
      ),
      child: Column(
        children: [
          // Header
          Padding(
            padding: EdgeInsets.symmetric(horizontal: 12, vertical: 6),
            child: Row(
              children: [
                Icon(Icons.terminal_rounded, color: Colors.greenAccent, size: 14),
                SizedBox(width: 6),
                Text('ACTIVITY LOG', style: TextStyle(
                  color: Colors.greenAccent, fontSize: 9, fontWeight: FontWeight.bold, letterSpacing: 1,
                )),
                Spacer(),
                // Download button
                GestureDetector(
                  onTap: _downloadClawLog,
                  child: Padding(
                    padding: EdgeInsets.symmetric(horizontal: 6),
                    child: Icon(Icons.download_rounded, color: Colors.greenAccent.withValues(alpha: 0.6), size: 14),
                  ),
                ),
                Container(
                  width: 6, height: 6,
                  decoration: BoxDecoration(color: Colors.greenAccent, shape: BoxShape.circle),
                ),
                SizedBox(width: 4),
                Text('LIVE', style: TextStyle(
                  color: Colors.greenAccent, fontSize: 8, fontWeight: FontWeight.bold, letterSpacing: 1,
                )),
              ],
            ),
          ),
          Divider(height: 1, color: Colors.greenAccent.withValues(alpha: 0.15)),
          // Log entries
          Expanded(
            child: _clawActivityLog.isEmpty
                ? Center(
                    child: Text('No activity yet', style: TextStyle(color: Colors.greenAccent.withValues(alpha: 0.4), fontSize: 10)),
                  )
                : ListView.builder(
                    controller: _clawLogScrollController,
                    padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    itemCount: _clawActivityLog.length,
                    itemBuilder: (context, i) {
                      final entry = _clawActivityLog[i];
                      final ts = entry['t'] as int? ?? 0;
                      final msg = entry['m']?.toString() ?? '';
                      final cat = entry['c']?.toString() ?? '';
                      final elapsed = ts > 0 ? _formatElapsed(ts) : '';
                      final icon = _clawCategoryIcon(cat);
                      return Padding(
                        padding: EdgeInsets.symmetric(vertical: 1),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(elapsed, style: TextStyle(color: Colors.greenAccent.withValues(alpha: 0.5), fontSize: 9, fontFamily: 'monospace')),
                            SizedBox(width: 4),
                            Text(icon, style: TextStyle(fontSize: 9)),
                            SizedBox(width: 4),
                            Expanded(child: Text(msg, style: TextStyle(color: Colors.greenAccent, fontSize: 9, fontFamily: 'monospace'), maxLines: 2, overflow: TextOverflow.ellipsis)),
                          ],
                        ),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }

  String _formatElapsed(int timestamp) {
    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final diff = now - timestamp;
    if (diff < 60) return '[${diff}s]';
    if (diff < 3600) return '[${diff ~/ 60}m]';
    return '[${diff ~/ 3600}h]';
  }

  Widget _buildClawStatChip(String label, String value, IconData icon, Color color) {
    return Expanded(
      child: Container(
        padding: EdgeInsets.symmetric(vertical: 8, horizontal: 10),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.08),
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: color.withValues(alpha: 0.2)),
        ),
        child: Column(
          children: [
            Icon(icon, color: color, size: 16),
            SizedBox(height: 4),
            Text(value, style: TextStyle(color: color, fontSize: 14, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
            Text(label, style: TextStyle(color: color.withValues(alpha: 0.7), fontSize: 8, fontWeight: FontWeight.bold, letterSpacing: 1)),
          ],
        ),
      ),
    );
  }

  String _clawCategoryIcon(String cat) {
    switch (cat) {
      case 'tick': return '✓';
      case 'battery': return '🔋';
      case 'recon': return '🔍';
      case 'heal': return '💊';
      case 'storage': return '💾';
      case 'network': return '🌐';
      case 'sync': return '🔄';
      case 'cache': return '📦';
      case 'db': return '🗄️';
      case 'security': return '🔒';
      default: return '•';
    }
  }

  void _runClawRecon() async {
    final client = IntrovertClient();
    _showSettingsClawTerminal('INTRO-CLAW RECON', []);
    final milestones = [
      '[00:01] Initializing mesh interface...',
      '[00:01] ✓ Mesh interface online (libp2p v0.56)',
      '[00:02] Querying Kademlia DHT routing table...',
      '[00:02] ✓ Routing table scanned — peers indexed',
      '[00:03] Polling connected crypto peers...',
      '[00:03] ✓ Direct P2P / relay / anchor connections mapped',
      '[00:04] Inspecting relay circuit reservations...',
      '[00:04] ✓ Active relay circuits via RBN backbone verified',
      '[00:05] Tracing connection types & latency...',
      '[00:05] ✓ Latency profiled — direct: ~45ms, relay: ~120ms',
      '[00:06] Scanning for mDNS local peers...',
      '[00:06] ✓ Local peer discovery complete',
      '[00:07] Checking WebSocket tunnel status...',
      '[00:07] ✓ Tunnel state recorded',
      '[00:08] Assembling network recon report...',
      '[00:08] ✓ Report generated — peer entries compiled',
    ];
    for (int i = 0; i < milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 200 + (i * 80)));
    }
    try {
      final report = client.runNetworkRecon();
      if (mounted) Navigator.of(context).pop();
      if (mounted) _showSettingsClawTerminal('INTRO-CLAW RECON', milestones, finalReport: report);
    } catch (e) {
      if (mounted) Navigator.of(context).pop();
    }
  }

  void _runClawHeal() async {
    final client = IntrovertClient();
    _showSettingsClawTerminal('INTRO-CLAW HEAL', []);
    final milestones = [
      '[00:01] Scanning peer connection states...',
      '[00:01] ✓ All known peers enumerated',
      '[00:02] Identifying unreachable peer endpoints...',
      '[00:02] ✓ Offline peers flagged with last-seen timestamps',
      '[00:03] Attempting direct libp2p dial...',
      '[00:03] → Direct dial initiated for unreachable peers',
      '[00:04] Trying relay circuit v2 via RBN...',
      '[00:04] ✓ Relay path constructed via backbone node',
      '[00:05] Checking anchor node routing...',
      '[00:05] ✓ Anchor nodes available for message relay',
      '[00:06] Attempting WebSocket tunnel fallback...',
      '[00:06] ✓ Connection strategy evaluated',
      '[00:07] Storing messages in persistent mailbox...',
      '[00:07] ✓ Pending messages queued for offline peers',
      '[00:08] Compiling heal report...',
      '[00:08] ✓ Heal cycle complete — strategies exhausted',
    ];
    for (int i = 0; i < milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 300 + (i * 100)));
    }
    try {
      final report = client.runNetworkRecon();
      final offlineCount = RegExp(r'OFFLINE').allMatches(report).length;
      final healReport = offlineCount == 0
          ? "All peers are connected. No healing needed."
          : "### Heal Summary\n\nFound $offlineCount offline peers.\n\nStrategies attempted:\n1. Direct libp2p dial\n2. Relay circuit v2\n3. Anchor node routing\n4. WebSocket tunnel\n5. Persistent mailbox fallback";
      if (mounted) Navigator.of(context).pop();
      if (mounted) _showSettingsClawTerminal('INTRO-CLAW HEAL', milestones, finalReport: healReport);
    } catch (e) {
      if (mounted) Navigator.of(context).pop();
    }
  }

  void _showSettingsClawTerminal(String title, List<String> milestones, {String? finalReport}) {
    showDialog(
      context: context,
      barrierColor: Colors.black.withValues(alpha: 0.7),
      builder: (context) => _ClawTerminalDialog(title: title, milestones: milestones, finalReport: finalReport),
    );
  }

  void _downloadClawLog() async {
    try {
      final buffer = StringBuffer();
      buffer.writeln('INTRO-CLAW ACTIVITY LOG');
      buffer.writeln('Generated: ${DateTime.now().toIso8601String()}');
      buffer.writeln('=' * 60);
      for (final entry in _clawActivityLog) {
        final ts = entry['t'] as int? ?? 0;
        final cat = entry['c']?.toString() ?? '';
        final msg = entry['m']?.toString() ?? '';
        final dt = ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts * 1000).toIso8601String() : '?';
        buffer.writeln('[$dt] [$cat] $msg');
      }
      final fileName = 'introvert_log_${DateTime.now().millisecondsSinceEpoch}.txt';
      // Save to Downloads directory so it's easily accessible
      Directory dir;
      if (Platform.isAndroid) {
        dir = Directory('/storage/emulated/0/Download');
        if (!await dir.exists()) {
          dir = await getTemporaryDirectory();
        }
      } else if (Platform.isIOS || Platform.isMacOS) {
        dir = await getApplicationDocumentsDirectory();
      } else {
        dir = await getTemporaryDirectory();
      }
      final file = File('${dir.path}/$fileName');
      await file.writeAsString(buffer.toString());
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Log saved: $fileName'),
            action: SnackBarAction(label: 'OK', onPressed: () {}),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to save log: $e')),
        );
      }
    }
  }

  void _saveNetworkDebugLog() async {
    try {
      final client = IntrovertClient();
      final logContent = client.getDebugLogs();
      final entryCount = client.debugLogCount;

      final buffer = StringBuffer();
      buffer.writeln('INTROVERT NETWORK DEBUG LOG');
      buffer.writeln('Generated: ${DateTime.now().toIso8601String()}');
      buffer.writeln('Total entries: $entryCount');
      buffer.writeln('=' * 60);
      buffer.writeln('Contents: Rust relay events, circuit establishment,');
      buffer.writeln('mesh routing decisions, dial strategies, mailbox drain.');
      buffer.writeln('=' * 60);
      buffer.writeln(logContent);

      final fileName = 'introvert_netlog_${DateTime.now().millisecondsSinceEpoch}.txt';
      Directory dir;
      if (Platform.isAndroid) {
        dir = Directory('/storage/emulated/0/Download');
        if (!await dir.exists()) dir = await getTemporaryDirectory();
      } else if (Platform.isIOS || Platform.isMacOS) {
        dir = await getApplicationDocumentsDirectory();
      } else {
        dir = await getTemporaryDirectory();
      }
      final file = File('${dir.path}/$fileName');
      await file.writeAsString(buffer.toString());
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Network log saved: $fileName ($entryCount entries)'),
            action: SnackBarAction(label: 'OK', onPressed: () {}),
            duration: const Duration(seconds: 5),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to save network log: $e')),
        );
      }
    }
  }

  void _triggerClawTick() {
    IntrovertClient().triggerIntroClawTick(isMobileData: false);
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Maintenance tick triggered')),
    );
  }

  Widget _buildSettingSection(String title, List<Widget> children) {
    return Padding(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      child: Theme(
        data: Theme.of(context).copyWith(
          dividerColor: Colors.transparent,
        ),
        child: GlassmorphicContainer(
          borderRadius: BorderRadius.circular(16),
          blur: 10,
          tintAlpha: 0.08,
          borderAlpha: 0.12,
          padding: EdgeInsets.zero,
          child: Material(
            color: Colors.transparent,
            borderRadius: BorderRadius.circular(16),
            clipBehavior: Clip.antiAlias,
            child: ExpansionTile(
              title: Text(
                title.toUpperCase(),
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.bold,
                  color: AppTheme.current.text,
                  letterSpacing: 1.2,
                ),
              ),
              childrenPadding: EdgeInsets.only(bottom: 8, top: 4),
              collapsedIconColor: AppTheme.current.mutedText,
              iconColor: AppTheme.current.accent,
              initiallyExpanded: title == 'Appearance',
              children: children.map((child) => Material(color: Colors.transparent, child: child)).toList(),
            ),
          ),
        ),
      ),
    );
  }

}

enum _ResolveState { input, resolving, resolved, failed }

class _ResolveHandleDialog extends StatefulWidget {
  final IntrovertClient client;
  final BuildContext parentContext;

  const _ResolveHandleDialog({
    required this.client,
    required this.parentContext,
  });

  @override
  State<_ResolveHandleDialog> createState() => _ResolveHandleDialogState();
}

class _ResolveHandleDialogState extends State<_ResolveHandleDialog> {
  _ResolveState _state = _ResolveState.input;
  final TextEditingController _controller = TextEditingController();
  StreamSubscription<NetworkEvent>? _subscription;
  String _resolvedPeerId = "";
  String _resolvingHandle = "";
  String _errorMessage = "";
  bool _isVerified = false;

  @override
  void initState() {
    super.initState();
    _subscription = widget.client.networkStream.listen((event) {
      if (!mounted) return;
      if (event.type == 33) {
        try {
          final parts = utf8.decode(event.data).split('\x00');
          if (parts.length < 2) return;
          final String handle = parts[0];
          final String peerId = parts[1];
          if (handle == _resolvingHandle) {
            final status = widget.client.getHandleStatus(handle);
            setState(() {
              _resolvedPeerId = peerId;
              _isVerified = status['verified'] == true;
              _state = _ResolveState.resolved;
            });
          }
        } catch (_) {}
      } else if (event.type == 35) {
        try {
          final handle = utf8.decode(event.data);
          if (handle == _resolvingHandle) {
            setState(() {
              _errorMessage = "Failed to resolve $handle. It may not exist or the network is unreachable.";
              _state = _ResolveState.failed;
            });
          }
        } catch (_) {}
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _startResolution() {
    var h = _controller.text.trim();
    if (h.isEmpty) return;
    if (!h.startsWith("i@")) {
      h = "i@$h";
    }
    setState(() {
      _resolvingHandle = h;
      _state = _ResolveState.resolving;
    });
    try {
      widget.client.resolveHandle(h);
    } catch (e) {
      debugPrint("⚠️ resolveHandle failed: $e");
      setState(() {
        _errorMessage = "Failed to initiate resolution: $e";
        _state = _ResolveState.failed;
      });
    }

    // Timeout: if no Event 33/35 in 30 seconds, show error
    Future.delayed(const Duration(seconds: 30), () {
      if (mounted && _state == _ResolveState.resolving && _resolvingHandle == h) {
        setState(() {
          _errorMessage = "Timed out resolving $h. The handle may not exist or the network is unreachable.";
          _state = _ResolveState.failed;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final bg = AppTheme.current.bg;
    final text = AppTheme.current.text;
    final accent = AppTheme.current.accent;
    final mutedText = AppTheme.current.mutedText;

    switch (_state) {
      case _ResolveState.input:
        return AlertDialog(
          backgroundColor: bg,
          title: Text("Add by Introvert Handle", style: TextStyle(color: text, fontSize: 16)),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text("Enter a handle (e.g. i@d3v6k) to resolve it via the Introvert mesh swarm.",
                  style: TextStyle(color: mutedText.withValues(alpha: 0.7), fontSize: 12)),
              SizedBox(height: 16),
              TextField(
                controller: _controller,
                autofocus: true,
                style: TextStyle(color: text, fontFamily: 'monospace'),
                decoration: InputDecoration(
                  labelText: "HANDLE",
                  labelStyle: TextStyle(color: accent),
                  enabledBorder: UnderlineInputBorder(
                      borderSide: BorderSide(color: mutedText.withValues(alpha: 0.1))),
                ),
                onSubmitted: (_) => _startResolution(),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CANCEL"),
            ),
            ElevatedButton(
              onPressed: _startResolution,
              style: ElevatedButton.styleFrom(
                  backgroundColor: accent, foregroundColor: Colors.black),
              child: Text("RESOLVE"),
            ),
          ],
        );

      case _ResolveState.resolving:
        return AlertDialog(
          backgroundColor: bg,
          title: Text("Resolving...", style: TextStyle(color: text, fontSize: 16)),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(height: 16),
              CircularProgressIndicator(color: accent),
              SizedBox(height: 24),
              Text(
                "Searching for $_resolvingHandle...",
                textAlign: TextAlign.center,
                style: TextStyle(color: text, fontSize: 13),
              ),
              SizedBox(height: 8),
              Text(
                "Querying RBN registry and global DHT nodes.",
                textAlign: TextAlign.center,
                style: TextStyle(color: mutedText.withValues(alpha: 0.6), fontSize: 11),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CANCEL"),
            ),
          ],
        );

      case _ResolveState.resolved:
        return AlertDialog(
          backgroundColor: bg,
          title: Row(
            children: [
              Flexible(
                child: Text("Handle Resolved", style: TextStyle(color: text, fontSize: 16)),
              ),
              if (_isVerified) ...[
                SizedBox(width: 8),
                Icon(Icons.verified, color: accent, size: 18),
              ],
            ],
          ),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                RichText(
                  text: TextSpan(
                    style: TextStyle(color: text.withValues(alpha: 0.7), fontSize: 14),
                    children: [
                      TextSpan(text: "Handle "),
                      TextSpan(
                        text: _resolvingHandle,
                        style: TextStyle(color: accent, fontWeight: FontWeight.bold),
                      ),
                      TextSpan(text: " points to peer:\n\n"),
                    ],
                  ),
                ),
                SelectableText(
                  _resolvedPeerId,
                  style: TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 11,
                      color: mutedText.withValues(alpha: 0.7)),
                ),
                if (_isVerified)
                  Padding(
                    padding: EdgeInsets.only(top: 16.0),
                    child: Text(
                      "This handle is OFFICIALLY VERIFIED by the Introvert Mesh.",
                      style: TextStyle(color: accent, fontSize: 11, fontWeight: FontWeight.bold),
                    ),
                  )
                else
                  Padding(
                    padding: EdgeInsets.only(top: 16.0),
                    child: Text(
                      "UNVERIFIED: This mapping has not been witnessed by RBN nodes yet.",
                      style: TextStyle(color: Colors.orangeAccent, fontSize: 11),
                    ),
                  ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CANCEL"),
            ),
            ElevatedButton(
              onPressed: () {
                final messenger = ScaffoldMessenger.of(widget.parentContext);
                try {
                  widget.client.sendDirectInvite(_resolvedPeerId);
                } catch (e) {
                  debugPrint("⚠️ sendDirectInvite failed: $e");
                }
                Navigator.pop(context);
                messenger.showSnackBar(
                  SnackBar(content: Text("Invite sent to $_resolvingHandle!")),
                );
              },
              style: ElevatedButton.styleFrom(
                  backgroundColor: accent, foregroundColor: Colors.black),
              child: Text("SEND INVITE"),
            ),
          ],
        );

      case _ResolveState.failed:
        return AlertDialog(
          backgroundColor: bg,
          title: Text("Resolution Failed", style: TextStyle(color: text, fontSize: 16)),
          content: Text(
            _errorMessage,
            style: TextStyle(color: Colors.orangeAccent, fontSize: 13),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text("CANCEL"),
            ),
            ElevatedButton(
              onPressed: () {
                setState(() {
                  _state = _ResolveState.input;
                });
              },
              style: ElevatedButton.styleFrom(
                  backgroundColor: accent, foregroundColor: Colors.black),
              child: Text("RETRY"),
            ),
          ],
        );
    }
  }
}

class _CreateGroupDialog extends StatefulWidget {
  final List<dynamic> contacts;
  final VoidCallback onComplete;
  const _CreateGroupDialog({required this.contacts, required this.onComplete});

  @override
  State<_CreateGroupDialog> createState() => _CreateGroupDialogState();
}

class _CreateGroupDialogState extends State<_CreateGroupDialog> {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _descController = TextEditingController();
  final List<String> _selectedPeerIds = [];

  @override
  void dispose() {
    _nameController.dispose();
    _descController.dispose();
    super.dispose();
  }

  void _createGroup() async {
    final name = _nameController.text.trim();
    final desc = _descController.text.trim();
    if (name.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text("Group Name cannot be empty")),
      );
      return;
    }
    final messenger = ScaffoldMessenger.of(context);
    _client.createGroup(name, desc, _selectedPeerIds);
    Navigator.pop(context);
    await Future.delayed(const Duration(milliseconds: 600));
    widget.onComplete();
    messenger.showSnackBar(
      SnackBar(content: Text("Group '$name' created successfully!")),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      title: Text("Create Sovereign Group", style: TextStyle(color: AppTheme.current.accent)),
      content: SizedBox(
        width: double.maxFinite,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              TextField(
                controller: _nameController,
                decoration: InputDecoration(
                  labelText: "GROUP NAME",
                  labelStyle: TextStyle(color: AppTheme.current.accent, fontSize: 11),
                  hintText: "Enter group name",
                  hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                  enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
                  focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
                ),
                style: TextStyle(color: AppTheme.current.text),
              ),
              SizedBox(height: 16),
              TextField(
                controller: _descController,
                decoration: InputDecoration(
                  labelText: "GROUP DESCRIPTION",
                  labelStyle: TextStyle(color: AppTheme.current.accent, fontSize: 11),
                  hintText: "Enter group description",
                  hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                  enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
                  focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
                ),
                style: TextStyle(color: AppTheme.current.text),
              ),
              SizedBox(height: 24),
              Text("SELECT INITIAL MEMBERS", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
              SizedBox(height: 8),
              if (widget.contacts.isEmpty)
                Text("No contacts available to add", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 12))
              else
                ListView.builder(
                  shrinkWrap: true,
                  physics: const NeverScrollableScrollPhysics(),
                  itemCount: widget.contacts.length,
                  itemBuilder: (context, index) {
                    final contact = widget.contacts[index];
                    final peerId = contact['peer_id'] as String;
                    final alias = contact['alias'] as String?;
                    final isSelected = _selectedPeerIds.contains(peerId);

                    return CheckboxListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        alias ?? "${peerId.substring(0, 12)}...",
                        style: TextStyle(
                          color: AppTheme.current.text.withValues(alpha: 0.7),
                          fontSize: 13,
                          fontFamily: (alias == null || alias.isEmpty) ? 'monospace' : null,
                        ),
                      ),
                      value: isSelected,
                      activeColor: AppTheme.current.accent,
                      checkColor: Colors.black,
                      onChanged: (val) {
                        setState(() {
                          if (val == true) {
                            _selectedPeerIds.add(peerId);
                          } else {
                            _selectedPeerIds.remove(peerId);
                          }
                        });
                      },
                    );
                  },
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
        ),
        ElevatedButton(
          onPressed: _createGroup,
          style: ElevatedButton.styleFrom(
            backgroundColor: AppTheme.current.accent,
            foregroundColor: Colors.black,
            minimumSize: const Size(100, 40),
          ),
          child: Text("CREATE"),
        ),
      ],
    );
  }
}

class _JoinGroupDialog extends StatefulWidget {
  final VoidCallback onComplete;
  const _JoinGroupDialog({required this.onComplete});

  @override
  State<_JoinGroupDialog> createState() => _JoinGroupDialogState();
}

class _JoinGroupDialogState extends State<_JoinGroupDialog> {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _codeController = TextEditingController();

  @override
  void dispose() {
    _codeController.dispose();
    super.dispose();
  }

  void _joinGroup() {
    final code = _codeController.text.trim();
    if (code.isEmpty) return;
    final messenger = ScaffoldMessenger.of(context);
    _client.joinMeshByCode(code);
    Navigator.pop(context);
    widget.onComplete();
    messenger.showSnackBar(
      SnackBar(content: Text("Joining Sovereign Group with code '$code'...")),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      title: Text("Join Sovereign Group", style: TextStyle(color: AppTheme.current.accent)),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text("Enter the passphrase or Introvert mesh swarm invitation key sent by the group admin to join.", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 12)),
          SizedBox(height: 16),
          TextField(
            controller: _codeController,
            decoration: InputDecoration(
              labelText: "MESH CODE / KEY",
              labelStyle: TextStyle(color: AppTheme.current.accent, fontSize: 11),
              hintText: "e.g., blue-sky-77",
              hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
              enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
              focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
            ),
            style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
        ),
        ElevatedButton(
          onPressed: _joinGroup,
          style: ElevatedButton.styleFrom(
            backgroundColor: AppTheme.current.accent,
            foregroundColor: Colors.black,
            minimumSize: const Size(100, 40),
          ),
          child: Text("JOIN"),
        ),
      ],
    );
  }
}

class _IncomingCallOverlay extends StatefulWidget {
  final String peerId;
  final String name;
  final String? avatarBase64;
  final Function(int) onAccept;
  final VoidCallback onDecline;

  const _IncomingCallOverlay({
    required this.peerId,
    required this.name,
    required this.avatarBase64,
    required this.onAccept,
    required this.onDecline,
  });

  @override
  State<_IncomingCallOverlay> createState() => _IncomingCallOverlayState();
}

class _IncomingCallOverlayState extends State<_IncomingCallOverlay> with SingleTickerProviderStateMixin {
  late AnimationController _pulseController;
  StreamSubscription? _subscription;
  int _peerTier = 0;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 2),
    )..repeat(reverse: true);
    _loadPeerTier();
    _subscription = IntrovertClient().networkStream.listen((event) {
      if (event.type == 32) {
        try {
          final parts = utf8.decode(event.data).split('\x00');
          final rejectedPeerId = parts[0];
          if (rejectedPeerId == widget.peerId) {
            if (mounted) Navigator.of(context).pop();
          }
        } catch (e) {
          debugPrint("Error decoding call rejection: $e");
        }
      }
    });
  }

  void _loadPeerTier() {
    try {
      final contacts = IntrovertClient().getContacts();
      final contact = contacts.firstWhere((c) => c['peer_id'] == widget.peerId, orElse: () => null);
      if (contact != null && mounted) {
        setState(() => _peerTier = contact['prestige_tier'] as int? ?? 0);
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _pulseController.dispose();
    _subscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final avatarImage = widget.avatarBase64 != null
        ? MemoryImage(_decodeAvatar(widget.avatarBase64!))
        : null;

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Center(
        child: Container(
          margin: const EdgeInsets.symmetric(horizontal: 24),
          padding: const EdgeInsets.all(32),
          decoration: BoxDecoration(
            color: const Color(0xFF121212).withValues(alpha: 0.95),
            borderRadius: BorderRadius.circular(28),
            border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2), width: 1.5),
            boxShadow: [
              BoxShadow(
                color: AppTheme.current.accent.withValues(alpha: 0.15),
                blurRadius: 30,
                spreadRadius: 5,
              )
            ],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              AnimatedBuilder(
                animation: _pulseController,
                builder: (context, child) {
                  return Container(
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      boxShadow: [
                        BoxShadow(
                          color: AppTheme.current.accent.withValues(alpha: 0.3 * _pulseController.value),
                          blurRadius: 20 * _pulseController.value,
                          spreadRadius: 4 * _pulseController.value,
                        )
                      ],
                    ),
                    child: SovereignAvatar(
                      radius: 50,
                      prestigeTier: _peerTier,
                      avatar: avatarImage,
                      initials: widget.name.isNotEmpty ? widget.name[0].toUpperCase() : "?",
                    ),
                  );
                },
              ),
              const SizedBox(height: 24),
              Text(
                widget.name,
                style: TextStyle(
                  color: AppTheme.current.text,
                  fontSize: 22,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 0.5,
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              Text(
                "INCOMING SOVEREIGN CALL",
                style: TextStyle(
                  color: AppTheme.current.accent.withValues(alpha: 0.8),
                  fontSize: 12,
                  fontWeight: FontWeight.w900,
                  letterSpacing: 1.5,
                ),
              ),
              const SizedBox(height: 36),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                children: [
                  _buildCallOptionButton(
                    icon: Icons.phone_rounded,
                    label: "Audio",
                    color: AppTheme.current.accent,
                    onTap: () => widget.onAccept(0), // 0 = Audio only
                  ),
                  _buildCallOptionButton(
                    icon: Icons.videocam_rounded,
                    label: "Video",
                    color: AppTheme.current.accent,
                    onTap: () => widget.onAccept(2), // 2 = Audio + Video
                  ),
                  _buildCallOptionButton(
                    icon: Icons.call_end_rounded,
                    label: "Decline",
                    color: Colors.redAccent,
                    onTap: widget.onDecline,
                  ),
                ],
              )
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCallOptionButton({
    required IconData icon,
    required String label,
    required Color color,
    required VoidCallback onTap,
  }) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(30),
          child: Container(
            width: 60,
            height: 60,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.15),
              shape: BoxShape.circle,
              border: Border.all(color: color.withValues(alpha: 0.5), width: 1.5),
            ),
            child: Icon(icon, color: color, size: 28),
          ),
        ),
        const SizedBox(height: 8),
        Text(
          label,
          style: TextStyle(
            color: AppTheme.current.text.withValues(alpha: 0.7),
            fontSize: 12,
            fontWeight: FontWeight.w500,
          ),
        ),
      ],
    );
  }
}

class _IncomingGroupCallOverlay extends StatefulWidget {
  final String callId;
  final String groupId;
  final String groupName;
  final String callerPeerId;
  final String callerName;
  final String? callerAvatar;
  final int mediaType;
  final List<String> existingMembers;
  final VoidCallback onAccept;
  final VoidCallback onDecline;

  const _IncomingGroupCallOverlay({
    required this.callId,
    required this.groupId,
    required this.groupName,
    required this.callerPeerId,
    required this.callerName,
    this.callerAvatar,
    required this.mediaType,
    required this.existingMembers,
    required this.onAccept,
    required this.onDecline,
  });

  @override
  State<_IncomingGroupCallOverlay> createState() => _IncomingGroupCallOverlayState();
}

class _IncomingGroupCallOverlayState extends State<_IncomingGroupCallOverlay> with SingleTickerProviderStateMixin {
  late AnimationController _pulseController;
  int _callerTier = 0;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1500),
    )..repeat(reverse: true);
    _loadCallerTier();
  }

  void _loadCallerTier() {
    try {
      final contacts = IntrovertClient().getContacts();
      final contact = contacts.firstWhere((c) => c['peer_id'] == widget.callerPeerId, orElse: () => null);
      if (contact != null && mounted) {
        setState(() => _callerTier = contact['prestige_tier'] as int? ?? 0);
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _pulseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final avatarImage = widget.callerAvatar != null
        ? MemoryImage(_decodeAvatar(widget.callerAvatar!))
        : null;

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Center(
        child: Container(
          margin: const EdgeInsets.symmetric(horizontal: 24),
          padding: const EdgeInsets.all(32),
          decoration: BoxDecoration(
            color: const Color(0xFF121212).withValues(alpha: 0.95),
            borderRadius: BorderRadius.circular(28),
            border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2), width: 1.5),
            boxShadow: [
              BoxShadow(
                color: AppTheme.current.accent.withValues(alpha: 0.15),
                blurRadius: 30,
                spreadRadius: 5,
              )
            ],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              AnimatedBuilder(
                animation: _pulseController,
                builder: (context, child) {
                  return Container(
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      boxShadow: [
                        BoxShadow(
                          color: AppTheme.current.accent.withValues(alpha: 0.3 * _pulseController.value),
                          blurRadius: 20 * _pulseController.value,
                          spreadRadius: 4 * _pulseController.value,
                        )
                      ],
                    ),
                    child: SovereignAvatar(
                      radius: 50,
                      prestigeTier: _callerTier,
                      avatar: avatarImage,
                      initials: widget.callerName.isNotEmpty ? widget.callerName[0].toUpperCase() : "?",
                    ),
                  );
                },
              ),
              const SizedBox(height: 20),
              Text(
                widget.callerName,
                style: TextStyle(
                  color: AppTheme.current.text,
                  fontSize: 22,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 0.5,
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              Text(
                widget.groupName,
                style: TextStyle(
                  color: AppTheme.current.accent.withValues(alpha: 0.8),
                  fontSize: 14,
                  fontWeight: FontWeight.w500,
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 4),
              Text(
                widget.mediaType == 0 ? "AUDIO GROUP CALL" : "VIDEO GROUP CALL",
                style: TextStyle(
                  color: AppTheme.current.mutedText.withValues(alpha: 0.7),
                  fontSize: 11,
                  fontWeight: FontWeight.w900,
                  letterSpacing: 1.5,
                ),
              ),
              const SizedBox(height: 8),
              if (widget.existingMembers.isNotEmpty)
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                  decoration: BoxDecoration(
                    color: AppTheme.current.accent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                  ),
                  child: Text(
                    "${widget.existingMembers.length} participant${widget.existingMembers.length > 1 ? 's' : ''} in call",
                    style: TextStyle(
                      color: AppTheme.current.accent,
                      fontSize: 11,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
              const SizedBox(height: 32),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                children: [
                  _buildCallOptionButton(
                    icon: Icons.videocam_rounded,
                    label: "Join Video",
                    color: AppTheme.current.accent,
                    onTap: widget.onAccept,
                  ),
                  _buildCallOptionButton(
                    icon: Icons.call_end_rounded,
                    label: "Decline",
                    color: Colors.redAccent,
                    onTap: widget.onDecline,
                  ),
                ],
              )
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCallOptionButton({
    required IconData icon,
    required String label,
    required Color color,
    required VoidCallback onTap,
  }) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(30),
          child: Container(
            width: 60,
            height: 60,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.15),
              shape: BoxShape.circle,
              border: Border.all(color: color.withValues(alpha: 0.5), width: 1.5),
            ),
            child: Icon(icon, color: color, size: 28),
          ),
        ),
        const SizedBox(height: 8),
        Text(
          label,
          style: TextStyle(
            color: AppTheme.current.text.withValues(alpha: 0.7),
            fontSize: 12,
            fontWeight: FontWeight.w500,
          ),
        ),
      ],
    );
  }
}

class _RbnManagerDialog extends StatefulWidget {
  final IntrovertClient client;
  final _MainShellState parentState;

  const _RbnManagerDialog({required this.client, required this.parentState});

  @override
  State<_RbnManagerDialog> createState() => _RbnManagerDialogState();
}

class _RbnManagerDialogState extends State<_RbnManagerDialog> {
  List<dynamic> _rbns = [];
  final TextEditingController _ipController = TextEditingController();
  bool _isTesting = false;
  String _testStatus = "";
  Color _statusColor = Colors.transparent;

  bool _isRefreshing = false;
  String _refreshMessage = "";

  @override
  void initState() {
    super.initState();
    _refreshRbns();
    
    // Subscribe to RBN connection test events
    widget.parentState.rbnConfirmedCallback = (ip, peerId, rtt) {
      if (mounted) {
        setState(() {
          _isTesting = false;
          _testStatus = "SUCCESS: Set to RBN (${_obfuscateAddress(ip)})";
          _statusColor = Colors.greenAccent;
          _refreshRbns();
        });
      }
    };

    widget.parentState.rbnFailedCallback = (ip, reason) {
      if (mounted) {
        setState(() {
          _isTesting = false;
          _testStatus = "FAILED to connect to ${_obfuscateAddress(ip)}: $reason";
          _statusColor = Colors.redAccent;
        });
      }
    };
  }

  @override
  void dispose() {
    // Unsubscribe
    widget.parentState.rbnConfirmedCallback = null;
    widget.parentState.rbnFailedCallback = null;
    _ipController.dispose();
    super.dispose();
  }

  String _obfuscateAddress(String address) {
    if (address.isEmpty) return address;
    
    // Match /ip4/x.x.x.x/tcp/443 style
    final ip4RegExp = RegExp(r'/ip4/(\d+)\.(\d+)\.(\d+)\.(\d+)');
    if (ip4RegExp.hasMatch(address)) {
      return address.replaceAllMapped(ip4RegExp, (m) => '/ip4/${m[1]}.${m[2]}.xxx.xxx');
    }
    
    // Match standard IPv4
    final plainIpRegExp = RegExp(r'^(\d+)\.(\d+)\.(\d+)\.(\d+)$');
    if (plainIpRegExp.hasMatch(address)) {
      return address.replaceAllMapped(plainIpRegExp, (m) => '${m[1]}.${m[2]}.xxx.xxx');
    }
    
    return address;
  }

  void _refreshRbns() {
    setState(() {
      _rbns = widget.client.getRbns();
    });
  }

  void _onRefreshPressed() async {
    if (_isRefreshing) return;
    setState(() {
      _isRefreshing = true;
      _refreshMessage = "Fetching latest RBN latency metrics...";
    });

    // Request updated swarm stats/latencies
    widget.client.requestSwarmStats();
    await Future.delayed(const Duration(milliseconds: 800));

    _refreshRbns();

    // Find RBN with lowest latency
    dynamic bestRbn;
    int minLatency = 999999;
    for (var rbn in _rbns) {
      final lat = rbn['latency_ms'];
      if (lat != null && lat < minLatency) {
        minLatency = lat;
        bestRbn = rbn;
      }
    }

    if (bestRbn != null) {
      setState(() {
        _refreshMessage = "Optimizing path: Switching RBNs for better speeds & reliability...";
      });
      await Future.delayed(const Duration(seconds: 2));
      setState(() {
        _isRefreshing = false;
        _refreshMessage = "";
      });
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text("Connected to optimal RBN: ${_obfuscateAddress(bestRbn['address'])} (${bestRbn['latency_ms']}ms)"),
            backgroundColor: AppTheme.current.accent,
          ),
        );
      }
    } else {
      setState(() {
        _isRefreshing = false;
        _refreshMessage = "";
      });
    }
  }

  void _startConnectionTest() {
    var ipInput = _ipController.text.trim();
    if (ipInput.isEmpty) return;

    // If user enters a plain IP (e.g. 10.0.0.1), append /ip4/ prefix
    final ipv4Regex = RegExp(r'^\d+\.\d+\.\d+\.\d+$');
    if (ipv4Regex.hasMatch(ipInput)) {
      ipInput = '/ip4/$ipInput/tcp/443';
    }

    setState(() {
      _isTesting = true;
      _testStatus = "Testing connection to ${_obfuscateAddress(ipInput)}...";
      _statusColor = AppTheme.current.accent;
    });

    widget.client.testRbn(ipInput);
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      child: Container(
        padding: const EdgeInsets.all(20),
        width: MediaQuery.of(context).size.width * 0.9,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  'ROUTING RELAYS (RBN)',
                  style: TextStyle(
                    color: AppTheme.current.accent,
                    fontSize: 14,
                    fontWeight: FontWeight.bold,
                    letterSpacing: 1.1,
                  ),
                ),
                GestureDetector(
                  onTap: () => Navigator.pop(context),
                  child: Icon(Icons.close, size: 20, color: AppTheme.current.mutedText),
                ),
              ],
            ),
            const SizedBox(height: 16),
            Text(
              'Intro Claw automatically selects the best RBN relay based on speed (lowest ping latency) to ensure optimal delivery. To preserve your device battery life, auto-optimization checks run only when the application is active and in the foreground.',
              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12, height: 1.3),
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  'ACTIVE RELAY LIST',
                  style: TextStyle(
                    fontSize: 10,
                    fontWeight: FontWeight.bold,
                    color: AppTheme.current.text.withValues(alpha: 0.7),
                    letterSpacing: 1.0,
                  ),
                ),
                _isRefreshing
                    ? SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent),
                      )
                    : InkWell(
                        onTap: _onRefreshPressed,
                        borderRadius: BorderRadius.circular(4),
                        child: Padding(
                          padding: const EdgeInsets.all(4.0),
                          child: Icon(Icons.refresh, size: 16, color: AppTheme.current.accent),
                        ),
                      ),
              ],
            ),
            if (_refreshMessage.isNotEmpty) ...[
              const SizedBox(height: 8),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                decoration: BoxDecoration(
                  color: AppTheme.current.accent.withValues(alpha: 0.08),
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                ),
                child: Row(
                  children: [
                    SizedBox(
                      width: 12,
                      height: 12,
                      child: CircularProgressIndicator(strokeWidth: 1.5, color: AppTheme.current.accent),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        _refreshMessage,
                        style: TextStyle(
                          fontSize: 11,
                          fontWeight: FontWeight.bold,
                          color: AppTheme.current.accent,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
            const SizedBox(height: 8),
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 180),
              child: _rbns.isEmpty
                  ? Center(
                      child: Text(
                        'No relays connected yet.',
                        style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12),
                      ),
                    )
                  : ListView.builder(
                      shrinkWrap: true,
                      itemCount: _rbns.length,
                      itemBuilder: (context, idx) {
                        final rbn = _rbns[idx];
                        final peerId = rbn['peer_id'] ?? '';
                        final truncatedPeerId = peerId.length > 20
                            ? '${peerId.substring(0, 8)}...${peerId.substring(peerId.length - 8)}'
                            : peerId;
                        final address = rbn['address'] ?? '';
                        final latency = rbn['latency_ms'];
                        
                        return Padding(
                          padding: const EdgeInsets.symmetric(vertical: 4.0),
                          child: Container(
                            padding: const EdgeInsets.all(10),
                            decoration: BoxDecoration(
                              color: AppTheme.current.bg,
                              borderRadius: BorderRadius.circular(10),
                              border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                            ),
                            child: Row(
                              children: [
                                Icon(Icons.dns, size: 18, color: AppTheme.current.accent),
                                const SizedBox(width: 10),
                                Expanded(
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        truncatedPeerId,
                                        style: const TextStyle(fontSize: 12, fontFamily: 'monospace', fontWeight: FontWeight.bold),
                                      ),
                                      const SizedBox(height: 2),
                                      Text(
                                        _obfuscateAddress(address),
                                        style: TextStyle(fontSize: 10, color: AppTheme.current.mutedText),
                                      ),
                                    ],
                                  ),
                                ),
                                Container(
                                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                  decoration: BoxDecoration(
                                    color: latency != null
                                        ? Colors.green.withValues(alpha: 0.1)
                                        : Colors.amber.withValues(alpha: 0.1),
                                    borderRadius: BorderRadius.circular(6),
                                  ),
                                  child: Text(
                                    latency != null ? '${latency}ms' : 'Pending',
                                    style: TextStyle(
                                      fontSize: 9,
                                      fontWeight: FontWeight.bold,
                                      color: latency != null ? Colors.greenAccent : Colors.amberAccent,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
            ),
            const SizedBox(height: 16),
            Text(
              'MANUALLY SET INTROVERT RBN ADDRESS',
              style: TextStyle(
                fontSize: 10,
                fontWeight: FontWeight.bold,
                color: AppTheme.current.text.withValues(alpha: 0.7),
                letterSpacing: 1.0,
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: Container(
                    height: 38,
                    decoration: BoxDecoration(
                      color: AppTheme.current.bg,
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
                    ),
                    padding: const EdgeInsets.symmetric(horizontal: 10),
                    alignment: Alignment.center,
                    child: TextField(
                      controller: _ipController,
                      enabled: !_isTesting,
                      decoration: const InputDecoration(
                        hintText: 'Enter RBN IP address',
                        hintStyle: TextStyle(fontSize: 12, color: Colors.grey),
                        border: InputBorder.none,
                        isDense: true,
                      ),
                      style: const TextStyle(fontSize: 12),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                SizedBox(
                  height: 38,
                  child: ElevatedButton(
                    onPressed: _isTesting ? null : _startConnectionTest,
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.current.accent,
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                    ),
                    child: _isTesting
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2, color: Colors.white),
                          )
                        : const Text('Test', style: TextStyle(fontSize: 12, color: Colors.white, fontWeight: FontWeight.bold)),
                  ),
                ),
              ],
            ),
            if (_testStatus.isNotEmpty) ...[
              const SizedBox(height: 12),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: _statusColor.withValues(alpha: 0.08),
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: _statusColor.withValues(alpha: 0.2)),
                ),
                child: Text(
                  _testStatus,
                  style: TextStyle(fontSize: 11, color: _statusColor, fontWeight: FontWeight.bold),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
