import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import '../../../theme/app_theme.dart';
import '../main_shell.dart';

class ConnectionRequestOverlay extends StatelessWidget {
  final String peerId;
  final String name;
  final String handle;
  final String? avatarBase64;
  final int prestigeTier;
  final VoidCallback onDecline;
  final VoidCallback onAccept;

  const ConnectionRequestOverlay({
    super.key,
    required this.peerId,
    required this.name,
    required this.handle,
    this.avatarBase64,
    required this.prestigeTier,
    required this.onDecline,
    required this.onAccept,
  });

  @override
  Widget build(BuildContext context) {
    final accent = AppTheme.current.accent;
    final text = AppTheme.current.text;
    final mutedText = AppTheme.current.mutedText;

    return Dialog(
      backgroundColor: Colors.transparent,
      elevation: 0,
      child: Container(
        margin: const EdgeInsets.symmetric(horizontal: 24),
        padding: const EdgeInsets.all(32),
        decoration: BoxDecoration(
          color: const Color(0xFF121212).withValues(alpha: 0.95),
          borderRadius: BorderRadius.circular(28),
          border: Border.all(color: accent.withValues(alpha: 0.2), width: 1.5),
          boxShadow: [
            BoxShadow(color: accent.withValues(alpha: 0.15), blurRadius: 40, spreadRadius: 5),
            BoxShadow(color: Colors.black.withValues(alpha: 0.5), blurRadius: 20, offset: Offset(0, 10)),
          ],
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Header
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(Icons.person_add_rounded, color: accent, size: 28),
                SizedBox(width: 12),
                Flexible(
                  child: Text("CONNECTION REQUEST", style: TextStyle(color: accent, fontSize: 14, fontWeight: FontWeight.bold, letterSpacing: 2), overflow: TextOverflow.ellipsis),
                ),
              ],
            ),
            SizedBox(height: 24),
            // Avatar
            _buildAvatar(),
            SizedBox(height: 16),
            // Name
            Text(name, style: TextStyle(color: text, fontWeight: FontWeight.bold, fontSize: 20)),
            SizedBox(height: 4),
            // Handle badge
            Container(
              padding: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
              decoration: BoxDecoration(
                color: accent.withValues(alpha: 0.1),
                borderRadius: BorderRadius.circular(12),
              ),
              child: Text(handle, style: TextStyle(color: accent, fontSize: 12, fontFamily: 'monospace')),
            ),
            SizedBox(height: 16),
            // Message
            Text(
              "wants to connect with you via the Sovereign Mesh.",
              textAlign: TextAlign.center,
              style: TextStyle(color: mutedText.withValues(alpha: 0.7), fontSize: 13),
            ),
            SizedBox(height: 32),
            // Buttons
            Row(
              children: [
                Expanded(
                  child: OutlinedButton(
                    onPressed: onDecline,
                    style: OutlinedButton.styleFrom(
                      side: BorderSide(color: mutedText.withValues(alpha: 0.3)),
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
                      padding: EdgeInsets.symmetric(vertical: 16),
                    ),
                    child: Text("DECLINE", style: TextStyle(color: mutedText.withValues(alpha: 0.7), fontWeight: FontWeight.bold)),
                  ),
                ),
                SizedBox(width: 16),
                Expanded(
                  child: ElevatedButton(
                    onPressed: onAccept,
                    style: ElevatedButton.styleFrom(
                      backgroundColor: accent,
                      foregroundColor: Colors.black,
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
                      padding: EdgeInsets.symmetric(vertical: 16),
                    ),
                    child: Text("ACCEPT", style: TextStyle(fontWeight: FontWeight.bold)),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildAvatar() {
    if (avatarBase64 != null && avatarBase64!.isNotEmpty) {
      try {
        final bytes = base64Decode(avatarBase64!);
        return CircleAvatar(
          radius: 60,
          backgroundImage: MemoryImage(bytes),
          backgroundColor: AppTheme.current.accent.withValues(alpha: 0.1),
        );
      } catch (_) {}
    }
    return CircleAvatar(
      radius: 60,
      backgroundColor: AppTheme.current.accent.withValues(alpha: 0.1),
      child: Text(
        name.isNotEmpty ? name[0].toUpperCase() : '?',
        style: TextStyle(fontSize: 40, color: AppTheme.current.accent, fontWeight: FontWeight.bold),
      ),
    );
  }
}
