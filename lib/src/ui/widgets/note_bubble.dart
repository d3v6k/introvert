import 'dart:io';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import '../../../theme/app_theme.dart';

class NoteBubble extends StatelessWidget {
  final String title;
  final String content;
  final String? imagePath;
  final bool isMe;
  final DateTime timestamp;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const NoteBubble({
    required this.title,
    required this.content,
    this.imagePath,
    required this.isMe,
    required this.timestamp,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: EdgeInsets.only(
        top: 8,
        left: 24,
        right: 24,
        bottom: (reactions != null && reactions!.isNotEmpty) ? 24 : 8,
      ),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Column(
        crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
        children: [
          Container(
            width: 270,
            decoration: BoxDecoration(
              gradient: LinearGradient(
                colors: [
                  AppTheme.current.accent.withValues(alpha: 0.15),
                  AppTheme.current.accent.withValues(alpha: 0.05),
                ],
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
              ),
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.25), width: 1),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Header
                Container(
                  padding: EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                  decoration: BoxDecoration(
                    border: Border(bottom: BorderSide(color: AppTheme.current.accent.withValues(alpha: 0.15), width: 0.5)),
                  ),
                  child: Row(
                    children: [
                      Icon(Icons.sticky_note_2_rounded, size: 14, color: AppTheme.current.accent),
                      SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          title,
                          style: TextStyle(
                            color: AppTheme.current.accent,
                            fontSize: 13,
                            fontWeight: FontWeight.bold,
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),

                // Image (if present)
                if (imagePath != null && imagePath!.isNotEmpty)
                  Builder(
                    builder: (context) {
                      final file = File(imagePath!);
                      if (file.existsSync()) {
                        return ClipRRect(
                          borderRadius: BorderRadius.vertical(bottom: Radius.circular(content.isEmpty ? 16 : 0)),
                          child: Image.file(
                            file,
                            width: 270,
                            height: 180,
                            fit: BoxFit.cover,
                            errorBuilder: (ctx, err, stack) => Container(
                              height: 60,
                              child: Center(child: Icon(Icons.broken_image, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 24)),
                            ),
                          ),
                        );
                      }
                      return SizedBox.shrink();
                    },
                  ),

                // Content
                if (content.isNotEmpty)
                  Padding(
                    padding: EdgeInsets.all(14),
                    child: Text(
                      content,
                      style: TextStyle(
                        color: AppTheme.current.text.withValues(alpha: 0.85),
                        fontSize: 13,
                        height: 1.5,
                      ),
                      maxLines: 6,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),

                // Footer
                Padding(
                  padding: EdgeInsets.symmetric(horizontal: 14, vertical: 6),
                  child: Text(
                    DateFormat('HH:mm').format(timestamp),
                    style: TextStyle(
                      color: AppTheme.current.mutedText.withValues(alpha: 0.5),
                      fontSize: 10,
                    ),
                  ),
                ),
              ],
            ),
          ),

          // Reactions
          if (reactions != null && reactions!.isNotEmpty)
            GestureDetector(
              onTap: onReactionTap,
              child: Container(
                margin: EdgeInsets.only(top: 4),
                padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: AppTheme.current.surface,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: _buildReactionsRow(),
                ),
              ),
            ),
        ],
      ),
    );
  }

  List<Widget> _buildReactionsRow() {
    final Map<String, int> counts = {};
    for (var r in reactions!) {
      final emoji = r['emoji']?.toString() ?? '';
      if (emoji.isNotEmpty) counts[emoji] = (counts[emoji] ?? 0) + 1;
    }
    return counts.entries.map((e) => Padding(
      padding: EdgeInsets.symmetric(horizontal: 2),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(e.key, style: TextStyle(fontSize: 12)),
          if (e.value > 1) ...[
            SizedBox(width: 2),
            Text("${e.value}", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 9, fontWeight: FontWeight.bold)),
          ],
        ],
      ),
    )).toList();
  }
}
