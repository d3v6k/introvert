import 'package:flutter/material.dart';
import '../src/ui/video_player.dart';

class CallScreen extends StatelessWidget {
  final String peerId;
  const CallScreen({required this.peerId, super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sovereign Video Call'),
        backgroundColor: Colors.black,
        foregroundColor: Colors.cyanAccent,
      ),
      body: Stack(
        children: [
          Positioned.fill(
            child: NativeVideoRenderer(peerId: peerId),
          ),
          Positioned(
            bottom: 40,
            left: 0,
            right: 0,
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                _buildCallButton(Icons.mic_off, Colors.white24, () {}),
                const SizedBox(width: 24),
                _buildCallButton(Icons.call_end, Colors.redAccent, () {
                  Navigator.pop(context);
                }),
                const SizedBox(width: 24),
                _buildCallButton(Icons.videocam_off, Colors.white24, () {}),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildCallButton(IconData icon, Color color, VoidCallback onTap) {
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: color,
          shape: BoxShape.circle,
        ),
        child: Icon(icon, color: Colors.white, size: 28),
      ),
    );
  }
}
