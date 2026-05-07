import 'package:flutter/material.dart';

class SecurityShield extends StatelessWidget {
  final bool isSecure;
  final String peerIdFingerprint;

  const SecurityShield({
    super.key,
    required this.isSecure,
    required this.peerIdFingerprint,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      decoration: BoxDecoration(
        color: isSecure ? Colors.green.withValues(alpha: 0.1) : Colors.orange.withValues(alpha: 0.1),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          color: isSecure ? Colors.greenAccent : Colors.orangeAccent,
          width: 0.5,
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            isSecure ? Icons.verified_user : Icons.warning_amber_rounded,
            size: 14,
            color: isSecure ? Colors.greenAccent : Colors.orangeAccent,
          ),
          const SizedBox(width: 8),
          Text(
            isSecure ? "E2EE ACTIVE" : "SIGNALING ONLY",
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: isSecure ? Colors.greenAccent : Colors.orangeAccent,
              fontFamily: 'monospace',
            ),
          ),
          if (isSecure) ...[
            const SizedBox(width: 8),
            const Text("|", style: TextStyle(color: Colors.white24)),
            const SizedBox(width: 8),
            Text(
              peerIdFingerprint,
              style: const TextStyle(
                fontSize: 10,
                color: Colors.white70,
                fontFamily: 'monospace',
              ),
            ),
          ],
        ],
      ),
    );
  }
}
