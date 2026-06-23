import 'package:flutter/material.dart';
import '../../native/introvert_client.dart';
import '../connection_diagnostics_overlay.dart';
import '../../../theme/app_theme.dart';

class NetworkOptimizationButton extends StatelessWidget {
  final String? peerId;
  final List<String>? groupMemberIds;
  final Color? color;
final double size;

  NetworkOptimizationButton({
    super.key,
    this.peerId,
    this.groupMemberIds,
    this.color,
    this.size = 20,
  });

  @override
  Widget build(BuildContext context) {
    final client = IntrovertClient();
    
    return IconButton(
      icon: Icon(Icons.wifi_tethering_rounded, color: color ?? AppTheme.current.accent, size: size),
      tooltip: peerId != null ? "Recheck Peer Connection" : (groupMemberIds != null ? "Mesh Optimisation" : "Network Optimisation"),
      onPressed: () {
        if (peerId != null) {
          // Individual Chat Case — safe, non-disruptive recheck
          client.recheckConnection(peerId!);
          showConnectionDiagnostics(
            context: context,
            peerId: peerId!,
            client: client,
          );
        } else if (groupMemberIds != null) {
          // Group Chat Case — safe, non-disruptive recheck of each member
          final myId = client.localPeerId;
          int count = 0;
          for (var pid in groupMemberIds!) {
            if (pid != myId) {
              client.recheckConnection(pid);
              count++;
            }
          }
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text(
                "MESH OPTIMISATION: Rechecking $count group members...",
                style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold),
              ),
              backgroundColor: const Color(0xFF001F2B),
              duration: const Duration(seconds: 4),
            ),
          );
        } else {
          // Global Case — requires confirmation as it causes a brief connectivity drop
          showDialog(
            context: context,
            builder: (ctx) => AlertDialog(
              backgroundColor: AppTheme.current.surface,
              title: Row(
                children: [
                  Icon(Icons.warning_amber_rounded, color: Colors.orangeAccent, size: 20),
                  SizedBox(width: 8),
                  Text("Network Hard Reset", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
                ],
              ),
              content: Text(
                "This will perform a full network reset: all active connections will be briefly dropped and re-established.\n\n"
                "Only use this if you are stuck OFFLINE and cannot reconnect normally.",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(ctx),
                  child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
                ),
                ElevatedButton(
                  onPressed: () {
                    Navigator.pop(ctx);
                    client.forceNetworkRefresh();
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(
                        content: Text(
                          "GLOBAL NETWORK OPTIMISATION: Performing hard reset...",
                          style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold),
                        ),
                        backgroundColor: Color(0xFF001F2B),
                        duration: Duration(seconds: 4),
                      ),
                    );
                  },
                  style: ElevatedButton.styleFrom(
                    backgroundColor: Colors.orangeAccent,
                    foregroundColor: Colors.black,
                  ),
                  child: Text("RESET NOW"),
                ),
              ],
            ),
          );
        }
      },
    );
  }
}
