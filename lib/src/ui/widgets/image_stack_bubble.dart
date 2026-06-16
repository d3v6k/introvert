import 'package:flutter/material.dart';
import 'dart:io';
import '../../native/introvert_client.dart';
import '../../../theme/app_theme.dart';

class ImageGroupProgress {
  final List<FileTransferProgress> images;
  
  ImageGroupProgress({required this.images});

  bool get isOutgoing => images.first.isOutgoing;
  String get peerId => images.first.peerId;
  int get startTimeMs => images.first.startTimeMs;
}

class ImageStackBubble extends StatelessWidget {
  final ImageGroupProgress group;
  final bool isMe;
  final VoidCallback onTap;
  final List<dynamic>? reactions;

  const ImageStackBubble({
    required this.group,
    required this.isMe,
    required this.onTap,
    this.reactions,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final images = group.images;
    final int count = images.length;
    
    // Calculate overall progress
    double totalProgress = 0.0;
    int completedCount = 0;
    for (var img in images) {
      if (img.isComplete || img.isVerified) {
        completedCount++;
        totalProgress += 1.0;
      } else {
        totalProgress += img.progress;
      }
    }
    final double avgProgress = totalProgress / count;
    final bool isAllComplete = completedCount == count;

    // Check if we should show a progress bar
    final String statusText = isAllComplete 
        ? "Verified • $count images" 
        : "Transferring • $completedCount/$count (${(avgProgress * 100).toInt()}%)";

    final Color stateColor = isAllComplete ? AppTheme.current.accent : AppTheme.current.accent;

    return Container(
      margin: EdgeInsets.only(
        top: 8,
        left: 24,
        right: 24,
        bottom: (reactions != null && reactions!.isNotEmpty) ? 24 : 8,
      ),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: GestureDetector(
        onTap: onTap,
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            // Outer wrapper with constraints
            SizedBox(
              width: 270,
              height: 270,
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  // CARD 3 (Bottom-most, only if count >= 3)
                  if (count >= 3)
                    Positioned.fill(
                      child: Transform.translate(
                        offset: const Offset(8, 8),
                        child: Transform.rotate(
                          angle: 0.05,
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(16),
                            child: Container(
                              decoration: BoxDecoration(
                                color: Colors.black.withValues(alpha: 0.3),
                                border: Border.all(color: Colors.white12, width: 1),
                                boxShadow: [
                                  BoxShadow(
                                    color: Colors.black.withValues(alpha: 0.2),
                                    blurRadius: 8,
                                    offset: const Offset(0, 4),
                                  ),
                                ],
                              ),
                              child: _buildImageWidget(images[2], height: 270, width: 270),
                            ),
                          ),
                        ),
                      ),
                    ),

                  // CARD 2 (Middle, if count >= 2)
                  if (count >= 2)
                    Positioned.fill(
                      child: Transform.translate(
                        offset: const Offset(4, 4),
                        child: Transform.rotate(
                          angle: -0.03,
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(16),
                            child: Container(
                              decoration: BoxDecoration(
                                color: Colors.black.withValues(alpha: 0.5),
                                border: Border.all(color: Colors.white24, width: 1),
                                boxShadow: [
                                  BoxShadow(
                                    color: Colors.black.withValues(alpha: 0.2),
                                    blurRadius: 8,
                                    offset: const Offset(0, 4),
                                  ),
                                ],
                              ),
                              child: _buildImageWidget(images[1], height: 270, width: 270),
                            ),
                          ),
                        ),
                      ),
                    ),

                  // CARD 1 (Top-most / Cover)
                  Positioned.fill(
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(16),
                      child: Container(
                        decoration: BoxDecoration(
                          color: AppTheme.current.surface.withValues(alpha: 0.9),
                          border: Border.all(
                            color: isAllComplete ? stateColor : AppTheme.current.text.withValues(alpha: 0.2),
                            width: isAllComplete ? 1.5 : 1,
                          ),
                          boxShadow: [
                            BoxShadow(
                              color: Colors.black.withValues(alpha: 0.3),
                              blurRadius: 12,
                              offset: const Offset(0, 6),
                            ),
                          ],
                        ),
                        child: Stack(
                          fit: StackFit.expand,
                          children: [
                            _buildImageWidget(images[0], height: 270, width: 270),
                            
                            // Bottom info overlay with text and progress bar
                            Positioned(
                              bottom: 0,
                              left: 0,
                              right: 0,
                              child: Container(
                                decoration: BoxDecoration(
                                  gradient: LinearGradient(
                                    colors: [Colors.transparent, Colors.black.withValues(alpha: 0.95)],
                                    begin: Alignment.topCenter,
                                    end: Alignment.bottomCenter,
                                  ),
                                ),
                                padding: const EdgeInsets.all(8),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(
                                      statusText,
                                      style: TextStyle(
                                        color: stateColor,
                                        fontSize: 9,
                                        fontWeight: FontWeight.bold,
                                      ),
                                    ),
                                    if (!isAllComplete) ...[
                                      const SizedBox(height: 4),
                                      ClipRRect(
                                        borderRadius: BorderRadius.circular(2),
                                        child: LinearProgressIndicator(
                                          value: avgProgress,
                                          backgroundColor: Colors.white12,
                                          valueColor: AlwaysStoppedAnimation<Color>(stateColor),
                                          minHeight: 2,
                                        ),
                                      ),
                                    ],
                                  ],
                                ),
                              ),
                            ),

                            // Total count badge in top-right corner
                            Positioned(
                              top: 8,
                              right: 8,
                              child: Container(
                                decoration: BoxDecoration(
                                  color: Colors.black.withValues(alpha: 0.75),
                                  borderRadius: BorderRadius.circular(12),
                                  border: Border.all(color: Colors.white24, width: 0.5),
                                ),
                                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    const Icon(Icons.collections_rounded, color: Colors.white, size: 10),
                                    const SizedBox(width: 4),
                                    Text(
                                      "$count",
                                      style: const TextStyle(
                                        color: Colors.white,
                                        fontSize: 10,
                                        fontWeight: FontWeight.bold,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            
            // Reactions overlay
            if (reactions != null && reactions!.isNotEmpty)
              Positioned(
                bottom: -12,
                left: isMe ? null : 8,
                right: isMe ? 8 : null,
                child: _buildReactionsRow(reactions!),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildReactionsRow(List<dynamic> reactions) {
    final Map<String, int> counts = {};
    for (var r in reactions) {
      final emoji = r['emoji']?.toString() ?? '';
      if (emoji.isNotEmpty) counts[emoji] = (counts[emoji] ?? 0) + 1;
    }

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: const Color(0xFF1E2430),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
        boxShadow: [
          BoxShadow(
            color: Colors.black.withValues(alpha: 0.3),
            blurRadius: 4,
            offset: const Offset(0, 2),
          ),
        ],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: counts.entries.map((e) => Padding(
          padding: const EdgeInsets.symmetric(horizontal: 2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(e.key, style: const TextStyle(fontSize: 12)),
              if (e.value > 1) ...[
                const SizedBox(width: 2),
                Text(
                  "${e.value}",
                  style: TextStyle(
                    color: AppTheme.current.text.withValues(alpha: 0.7),
                    fontSize: 9,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ],
            ],
          ),
        )).toList(),
      ),
    );
  }

  Widget _buildImageWidget(FileTransferProgress img, {required double height, required double width}) {
    final bool hasLocalFile = img.localPath != null && File(img.localPath!).existsSync();
    if (hasLocalFile) {
      return Image.file(
        File(img.localPath!),
        height: height,
        width: width,
        fit: BoxFit.cover,
        errorBuilder: (context, error, stackTrace) => Container(
          color: Colors.black38,
          child: const Icon(Icons.broken_image, color: Colors.white24, size: 24),
        ),
      );
    } else {
      return Container(
        color: Colors.black45,
        height: height,
        width: width,
        child: Center(
          child: SizedBox(
            width: 20,
            height: 20,
            child: CircularProgressIndicator(
              value: img.progress > 0 && img.progress < 1.0 ? img.progress : null,
              strokeWidth: 2,
              valueColor: AlwaysStoppedAnimation<Color>(AppTheme.current.accent),
            ),
          ),
        ),
      );
    }
  }
}
