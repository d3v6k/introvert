import 'package:flutter/material.dart';
import 'dart:io';
import 'dart:convert';
import '../../native/introvert_client.dart';
import '../../../theme/app_theme.dart';

class ImageGroupProgress {
  final List<FileTransferProgress> images;
  
  ImageGroupProgress({required this.images});

  bool get isOutgoing => images.first.isOutgoing;
  String get peerId => images.first.peerId;
  int get startTimeMs => images.first.startTimeMs;

  DateTime get startDateTime => startTimeMs > 946684800000
      ? DateTime.fromMillisecondsSinceEpoch(startTimeMs)
      : DateTime.now();
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

    final Color stateColor = AppTheme.current.accent;

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
            Container(
              width: 270,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(16),
                border: Border.all(
                  color: isAllComplete ? stateColor.withValues(alpha: 0.4) : AppTheme.current.text.withValues(alpha: 0.1),
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
              child: ClipRRect(
                borderRadius: BorderRadius.circular(16),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _buildGrid(images, count),
                    // Bottom info bar
                    Container(
                      color: AppTheme.current.surface.withValues(alpha: 0.95),
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                      child: Row(
                        children: [
                          Icon(Icons.collections_rounded, color: stateColor, size: 12),
                          const SizedBox(width: 4),
                          Text(
                            "$count photos",
                            style: TextStyle(
                              color: stateColor,
                              fontSize: 10,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                          const Spacer(),
                          Text(
                            isAllComplete ? "Verified" : "${(avgProgress * 100).toInt()}%",
                            style: TextStyle(
                              color: isAllComplete ? stateColor : AppTheme.current.mutedText,
                              fontSize: 9,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                          if (!isAllComplete) ...[
                            const SizedBox(width: 6),
                            SizedBox(
                              width: 40,
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(2),
                                child: LinearProgressIndicator(
                                  value: avgProgress,
                                  backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.15),
                                  valueColor: AlwaysStoppedAnimation<Color>(stateColor),
                                  minHeight: 2,
                                ),
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
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

  Widget _buildGrid(List<FileTransferProgress> images, int count) {
    const double gap = 2;
    const double gridWidth = 270;
    
    if (count == 1) {
      return _buildGridCell(images[0], height: 200, width: gridWidth);
    } else if (count == 2) {
      return SizedBox(
        height: 180,
        child: Row(
          children: [
            Expanded(child: _buildGridCell(images[0], height: 180, width: gridWidth / 2)),
            const SizedBox(width: gap),
            Expanded(child: _buildGridCell(images[1], height: 180, width: gridWidth / 2)),
          ],
        ),
      );
    } else if (count == 3) {
      return SizedBox(
        height: 200,
        child: Row(
          children: [
            Expanded(
              child: _buildGridCell(images[0], height: 200, width: gridWidth / 2),
            ),
            const SizedBox(width: gap),
            Expanded(
              child: Column(
                children: [
                  Expanded(child: _buildGridCell(images[1], height: 99, width: gridWidth / 2)),
                  const SizedBox(height: gap),
                  Expanded(child: _buildGridCell(images[2], height: 99, width: gridWidth / 2)),
                ],
              ),
            ),
          ],
        ),
      );
    } else {
      // 4+ images: 2x2 grid, with +N overlay on the 4th cell
      final int overflow = count - 4;
      return SizedBox(
        height: 220,
        child: Column(
          children: [
            Expanded(
              child: Row(
                children: [
                  Expanded(child: _buildGridCell(images[0], height: 109, width: gridWidth / 2)),
                  const SizedBox(width: gap),
                  Expanded(child: _buildGridCell(images[1], height: 109, width: gridWidth / 2)),
                ],
              ),
            ),
            const SizedBox(height: gap),
            Expanded(
              child: Row(
                children: [
                  Expanded(child: _buildGridCell(images[2], height: 109, width: gridWidth / 2)),
                  const SizedBox(width: gap),
                  Expanded(
                    child: Stack(
                      children: [
                        _buildGridCell(images[3], height: 109, width: gridWidth / 2),
                        if (overflow > 0)
                          Positioned.fill(
                            child: Container(
                              decoration: BoxDecoration(
                                color: Colors.black.withValues(alpha: 0.55),
                                borderRadius: BorderRadius.circular(0),
                              ),
                              child: Center(
                                child: Text(
                                  "+$overflow",
                                  style: const TextStyle(
                                    color: Colors.white,
                                    fontSize: 22,
                                    fontWeight: FontWeight.bold,
                                  ),
                                ),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      );
    }
  }

  Widget _buildGridCell(FileTransferProgress img, {required double height, required double width}) {
    final bool hasLocalFile = img.localPath != null && File(img.localPath!).existsSync();
    
    Widget imageWidget;
    if (hasLocalFile) {
      imageWidget = Image.file(
        File(img.localPath!),
        height: height,
        width: width,
        fit: BoxFit.cover,
        errorBuilder: (context, error, stackTrace) => _buildPlaceholder(height, width),
      );
    } else if (img.thumbnail != null && img.thumbnail!.isNotEmpty) {
      try {
        final bytes = base64Decode(img.thumbnail!);
        imageWidget = Image.memory(
          bytes,
          height: height,
          width: width,
          fit: BoxFit.cover,
          errorBuilder: (context, error, stackTrace) => _buildPlaceholder(height, width),
        );
      } catch (_) {
        imageWidget = _buildPlaceholder(height, width);
      }
    } else {
      imageWidget = _buildPlaceholder(height, width);
    }

    return imageWidget;
  }

  Widget _buildPlaceholder(double height, double width) {
    return Container(
      color: AppTheme.current.text.withValues(alpha: 0.06),
      height: height,
      width: width,
      child: Center(
        child: SizedBox(
          width: 18,
          height: 18,
          child: CircularProgressIndicator(
            strokeWidth: 2,
            valueColor: AlwaysStoppedAnimation<Color>(AppTheme.current.accent.withValues(alpha: 0.4)),
          ),
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
}
