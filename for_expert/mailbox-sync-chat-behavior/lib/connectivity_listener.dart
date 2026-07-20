import 'dart:async';
import 'package:flutter/material.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'src/native/introvert_client.dart';

/// Listens for connectivity changes and informs the native Intro‑Claw.
/// Notifications are handled by main_shell.dart with rate limiting.
class ConnectivityListener extends StatefulWidget {
  final Widget child;
  const ConnectivityListener({required this.child, Key? key}) : super(key: key);

  @override
  State<ConnectivityListener> createState() => _ConnectivityListenerState();
}

class _ConnectivityListenerState extends State<ConnectivityListener> {
  late final StreamSubscription<List<ConnectivityResult>> _subscription;

  @override
  void initState() {
    super.initState();
    final client = IntrovertClient();
    _subscription = Connectivity().onConnectivityChanged.listen((results) {
      // No VPN detection — connectivity_plus VPN detection has false positives.
      // Let the resilience ladder handle tunnel activation (60s no peers → activate tunnel).
      final result = results.isNotEmpty ? results.first : ConnectivityResult.none;
      // Inform native layer — no UI notifications here (handled by main_shell with rate limiting)
      client.setConnectivityType(result);
      if (result == ConnectivityResult.none) {
        // Attempt to restart networking silently.
        client.startNetwork();
      }
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
