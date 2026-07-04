import 'package:flutter/material.dart';
import '../../../theme/app_theme.dart';

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
      padding: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      decoration: BoxDecoration(
        color: isSecure ? AppTheme.current.accent.withValues(alpha: 0.1) : Colors.orange.withValues(alpha: 0.1),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          color: isSecure ? AppTheme.current.accent : Colors.orangeAccent,
          width: 0.5,
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            isSecure ? Icons.verified_user : Icons.warning_amber_rounded,
            size: 14,
            color: isSecure ? AppTheme.current.accent : Colors.orangeAccent,
          ),
          SizedBox(width: 8),
          Text(
            isSecure ? "E2EE ACTIVE" : "SIGNALING ONLY",
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: isSecure ? AppTheme.current.accent : Colors.orangeAccent,
              fontFamily: 'monospace',
            ),
          ),
          if (isSecure) ...[
            SizedBox(width: 8),
            Text("|", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
            SizedBox(width: 8),
            Text(
              peerIdFingerprint,
              style: TextStyle(
                fontSize: 10,
                color: AppTheme.current.text.withValues(alpha: 0.7),
                fontFamily: 'monospace',
              ),
            ),
          ],
        ],
      ),
    );
  }
}
