import 'dart:async';
import 'package:flutter/material.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'src/native/introvert_client.dart';

/// Listens for connectivity changes and informs the native Intro‑Claw.
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
      final hasVpn = results.contains(ConnectivityResult.vpn);
      final result = hasVpn 
          ? ConnectivityResult.vpn 
          : (results.isNotEmpty ? results.first : ConnectivityResult.none);
      client.setConnectivityType(result);
      if (result == ConnectivityResult.none) {
        // Discreet user notification.
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Network issue detected… Intro‑Claw resolving'),
            duration: Duration(seconds: 4),
          ),
        );
        // Attempt to restart networking.
        client.startNetwork();
      } else if (result == ConnectivityResult.vpn) {
        // Discreet user notification for VPN adaptation.
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('VPN connection detected… Intro‑Claw adapting to tunnel‑only mode'),
            duration: Duration(seconds: 4),
          ),
        );
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
