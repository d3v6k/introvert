import 'package:flutter/material.dart';
import 'dart:ui' as ui;
import 'theme/app_theme.dart';

class SovereignAvatar extends StatelessWidget {
  final double radius;
  final ImageProvider? avatar;
  final String? initials;
  final double? balance;
  final bool isSuperActive;

  const SovereignAvatar({
    this.radius = 20,
    this.avatar,
    this.initials,
    this.balance,
    this.isSuperActive = false,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Container(
          width: radius * 2,
          height: radius * 2,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            border: Border.all(
              color: isSuperActive ? Colors.greenAccent : AppTheme.current.accent.withValues(alpha: 0.3),
              width: isSuperActive ? 2 : 1,
            ),
            boxShadow: [
              BoxShadow(
                color: (isSuperActive ? Colors.greenAccent : AppTheme.current.accent).withValues(alpha: 0.1),
                blurRadius: 4,
              ),
            ],
          ),
          child: CircleAvatar(
            radius: radius,
            backgroundColor: AppTheme.current.surface,
            backgroundImage: avatar ?? const AssetImage('assets/images/default_avatar.png'),
            child: (avatar == null && initials != null && initials!.isNotEmpty)
                ? Text(
                    initials!,
                    style: TextStyle(
                      color: AppTheme.current.accent,
                      fontSize: radius * 0.8,
                      fontWeight: FontWeight.bold,
                    ),
                  )
                : null,
          ),
        ),
        if (balance != null && balance! > 0)
          Positioned(
            right: 0,
            bottom: 0,
            child: Container(
              padding: EdgeInsets.all(2),
              decoration: BoxDecoration(color: Colors.black, shape: BoxShape.circle),
              child: Icon(Icons.bolt, color: Colors.yellowAccent, size: 10),
            ),
          ),
      ],
    );
  }
}

class GlassmorphicBubble extends StatelessWidget {
  final String content;
  final bool isMe;
  final DateTime? timestamp;
  final int status; // 0=Sent, 1=Delivered, 2=Read
  final dynamic replyTo;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;
  final VoidCallback? onReplyTap;
  final ImageProvider? replyAvatar;

  const GlassmorphicBubble({
    required this.content,
    required this.isMe,
    this.timestamp,
    this.status = 1,
    this.replyTo,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    this.onReplyTap,
    this.replyAvatar,
    super.key,
  });

  String _formatTime(DateTime? dt) {
    if (dt == null) return "--:--";
    final hour = dt.hour.toString().padLeft(2, '0');
    final minute = dt.minute.toString().padLeft(2, '0');
    return "$hour:$minute";
  }

  Widget _buildStatusTicks() {
    if (!isMe) return SizedBox.shrink();
    
    IconData icon = Icons.done_rounded; // Default: Single Tick (Sent)
    Color color = AppTheme.current.text.withValues(alpha: 0.3);
    
    if (status == 1) {
      icon = Icons.done_all_rounded; // Double Grey (Delivered)
    } else if (status == 2) {
      icon = Icons.done_all_rounded; // Double Blue (Read)
      color = AppTheme.current.accent;
    }

    return Icon(icon, size: 10, color: color);
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
      margin: EdgeInsets.only(bottom: (reactions != null && reactions!.isNotEmpty) ? 14 : 0),
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Column(
            crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
            children: [
              ClipRRect(
                borderRadius: BorderRadius.circular(16).copyWith(
                  bottomRight: isMe ? const Radius.circular(0) : null,
                  bottomLeft: !isMe ? const Radius.circular(0) : null,
                ),
                child: BackdropFilter(
                  filter: ui.ImageFilter.blur(sigmaX: 10, sigmaY: 10),
                  child: Container(
                    padding: EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: isMe ? AppTheme.current.accent.withValues(alpha: 0.1) : AppTheme.current.text.withValues(alpha: 0.05),
                      border: Border.all(color: isMe ? AppTheme.current.accent.withValues(alpha: 0.2) : AppTheme.current.mutedText.withValues(alpha: 0.1)),
                      borderRadius: BorderRadius.circular(16).copyWith(
                        bottomRight: isMe ? const Radius.circular(0) : null,
                        bottomLeft: !isMe ? const Radius.circular(0) : null,
                      ),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        if (replyTo != null) ...[
                          GestureDetector(
                            onTap: onReplyTap,
                            child: _buildReplyPreview(replyTo),
                          ),
                          SizedBox(height: 8),
                        ],
                        if (content == "[DELETED_BY_ADMIN]")
                          Text(
                            "DELETED BY ADMIN",
                            style: TextStyle(color: Colors.redAccent.withValues(alpha: 0.7), fontSize: 11, fontStyle: FontStyle.italic, fontWeight: FontWeight.bold, letterSpacing: 0.5),
                          )
                        else
                          Text(
                            content,
                            style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                          ),
                        SizedBox(height: 4),
                        Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              _formatTime(timestamp),
                              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 8),
                            ),
                            if (isMe) ...[
                              SizedBox(width: 4),
                              _buildStatusTicks(),
                            ],
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
          // Reaction Overlay
          if (reactions != null && reactions!.isNotEmpty)
            Positioned(
              bottom: -10,
              left: isMe ? null : 8,
              right: isMe ? 8 : null,
              child: _buildReactionsRow(),
            ),
        ],
      ),
    );
  }

  Widget _buildReactionsRow() {
    final Map<String, int> counts = {};
    for (var r in reactions!) {
       final emoji = r['emoji']?.toString() ?? '';
       if (emoji.isNotEmpty) counts[emoji] = (counts[emoji] ?? 0) + 1;
    }

    return GestureDetector(
      onTap: onReactionTap,
      child: Container(
        padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
        decoration: BoxDecoration(
          color: AppTheme.current.surface,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          boxShadow: [BoxShadow(color: Colors.black.withValues(alpha: 0.3), blurRadius: 4, offset: const Offset(0, 2))],
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: counts.entries.map((e) => Padding(
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
          )).toList(),
        ),
      ),
    );
  }

  Widget _buildReplyPreview(dynamic parent) {
    String pContent = "";
    String pSender = "Peer";
    
    try {
      if (parent is List && parent.length > 2) {
        pContent = parent[2].toString();
        pSender = parent[1].toString();
      } else {
        pContent = parent.content?.toString() ?? "";
        pSender = parent.isMe == true ? "me" : "peer";
      }
    } catch (_) {}

    return Container(
      padding: EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: AppTheme.current.text.withValues(alpha: 0.05),
        borderRadius: BorderRadius.circular(8),
        border: Border(left: BorderSide(color: AppTheme.current.accent, width: 3)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (replyAvatar != null) ...[
            SovereignAvatar(radius: 15, avatar: replyAvatar),
            SizedBox(width: 8),
          ],
          Flexible(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  pSender.toUpperCase(),
                  style: TextStyle(color: AppTheme.current.accent, fontSize: 9, fontWeight: FontWeight.bold),
                ),
                Text(
                  pContent,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class SovereignWallpaper extends StatelessWidget {
  const SovereignWallpaper({super.key});

  @override
  Widget build(BuildContext context) {
    final double h = MediaQuery.of(context).size.height * 0.6;
    return Positioned(
      bottom: h * 0.2, // Raised by 20% of the image height as requested
      left: 0,
      right: 0,
      height: h,
      child: IgnorePointer(
        child: Opacity(
          opacity: 0.25,
          child: Image.asset(
            'assets/images/introvert_wallpaper.png',
            fit: BoxFit.fitWidth,
            alignment: Alignment.bottomCenter,
          ),
        ),
      ),
    );
  }
}
