import 'package:flutter/material.dart';

/// Shared widgets for call screens (1:1 and group).
class CallWidgets {
  /// Control button used in both call screens.
  static Widget controlButton({
    required IconData icon,
    required String label,
    required bool active,
    required VoidCallback onTap,
    double size = 56,
    double iconSize = 24,
    double fontSize = 10,
  }) {
    return GestureDetector(
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: size,
            height: size,
            decoration: BoxDecoration(
              color: active
                  ? Colors.redAccent.withValues(alpha: 0.8)
                  : Colors.white.withValues(alpha: 0.15),
              shape: BoxShape.circle,
              border: Border.all(color: Colors.white.withValues(alpha: 0.1)),
            ),
            child: Icon(icon, color: Colors.white, size: iconSize),
          ),
          SizedBox(height: size == 56 ? 6 : 4),
          Text(label, style: TextStyle(color: Colors.white70, fontSize: fontSize)),
        ],
      ),
    );
  }

  /// Hang up button used in both call screens.
  static Widget hangUpButton({required VoidCallback onTap, double size = 70}) {
    return GestureDetector(
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: size,
            height: size,
            decoration: const BoxDecoration(
              color: Colors.redAccent,
              shape: BoxShape.circle,
              boxShadow: [BoxShadow(color: Colors.redAccent, blurRadius: 16, spreadRadius: 2)],
            ),
            child: Icon(Icons.call_end_rounded, color: Colors.white, size: size * 0.43),
          ),
          const SizedBox(height: 6),
          const Text('End', style: TextStyle(color: Colors.white70, fontSize: 10)),
        ],
      ),
    );
  }

  /// Format call duration.
  static String formatDuration(Duration d) {
    final h = d.inHours;
    final m = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final s = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return h > 0 ? '$h:$m:$s' : '$m:$s';
  }
}
