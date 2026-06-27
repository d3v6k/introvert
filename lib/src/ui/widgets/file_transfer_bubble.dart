import 'package:flutter/material.dart';
import 'dart:ui';
import 'dart:io';
import 'dart:convert';
import 'dart:async';
import 'package:audioplayers/audioplayers.dart';
import '../../native/introvert_client.dart';
import 'package:open_file/open_file.dart';
import 'package:path_provider/path_provider.dart';
import 'package:pdf_render_maintained/pdf_render_maintained.dart';
import 'package:flutter_video_thumbnail_plus/flutter_video_thumbnail_plus.dart';
import '../../../theme/app_theme.dart';
import '../../../views/media_gallery_viewer.dart';

class FileTransferBubble extends StatefulWidget {
  final FileTransferProgress progress;
  final bool isMe;
  final VoidCallback? onTap;
  final List<dynamic>? reactions;
  final List<dynamic>? allMessages;

  const FileTransferBubble({
    required this.progress,
    required this.isMe,
    this.onTap,
    this.reactions,
    this.allMessages,
    super.key,
  });

  @override
  State<FileTransferBubble> createState() => _FileTransferBubbleState();
}

class _FileTransferBubbleState extends State<FileTransferBubble> {
  String? _thumbnailPath;
  bool _isGenerating = false;
  String? _lastLoadedTransferId;

  // Audio Player State
  AudioPlayer? _audioPlayer;
  bool _isPlaying = false;
  Duration _audioPosition = Duration.zero;
  Duration _audioDuration = Duration.zero;
  StreamSubscription? _audioPositionSub;
  StreamSubscription? _audioDurationSub;
  StreamSubscription? _audioStateSub;
  StreamSubscription? _audioCompleteSub;

  @override
  void initState() {
    super.initState();
    _loadThumbnail();
    _initAudioPlayerIfNeeded();
  }

  @override
  void didUpdateWidget(covariant FileTransferBubble oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.progress.transferId != widget.progress.transferId ||
        oldWidget.progress.localPath != widget.progress.localPath ||
        oldWidget.progress.isOutgoing != widget.progress.isOutgoing ||
        oldWidget.progress.isVerified != widget.progress.isVerified) {
      _loadThumbnail();
      _initAudioPlayerIfNeeded();
    }
  }

  @override
  void dispose() {
    _audioPositionSub?.cancel();
    _audioDurationSub?.cancel();
    _audioStateSub?.cancel();
    _audioCompleteSub?.cancel();
    _audioPlayer?.dispose();
    super.dispose();
  }

  void _initAudioPlayerIfNeeded() {
    final progress = widget.progress;
    final String ext = progress.filename.split('.').last.toLowerCase();
    final bool isAudio = progress.mimeType.startsWith('audio/') ||
        ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac'].contains(ext);
    final bool isVerified = progress.isVerified;
    final String? localPath = progress.localPath;

    if (isAudio && isVerified && localPath != null && File(localPath).existsSync()) {
      if (_audioPlayer == null) {
        _audioPlayer = AudioPlayer();
        
        _audioPositionSub = _audioPlayer!.onPositionChanged.listen((pos) {
          if (mounted) {
            setState(() {
              _audioPosition = pos;
            });
          }
        });

        _audioDurationSub = _audioPlayer!.onDurationChanged.listen((dur) {
          if (mounted) {
            setState(() {
              _audioDuration = dur;
            });
          }
        });

        _audioStateSub = _audioPlayer!.onPlayerStateChanged.listen((state) {
          if (mounted) {
            setState(() {
              _isPlaying = state == PlayerState.playing;
            });
          }
        });

        _audioCompleteSub = _audioPlayer!.onPlayerComplete.listen((_) {
          if (mounted) {
            setState(() {
              _audioPosition = Duration.zero;
              _isPlaying = false;
            });
          }
        });
      }
    }
  }

  void _toggleAudioPlayback() async {
    if (_audioPlayer == null) return;
    if (_isPlaying) {
      await _audioPlayer!.pause();
    } else {
      final String? localPath = widget.progress.localPath;
      if (localPath != null && File(localPath).existsSync()) {
        await _audioPlayer!.play(DeviceFileSource(localPath));
      }
    }
  }

  void _stopAudioPlayback() async {
    if (_audioPlayer == null) return;
    await _audioPlayer!.stop();
    setState(() {
      _audioPosition = Duration.zero;
      _isPlaying = false;
    });
  }

  String _formatDuration(Duration d) {
    final minutes = d.inMinutes.remainder(60).toString();
    final seconds = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return "$minutes:$seconds";
  }

  Widget _buildAudioPlayerWidget(Color stateColor) {
    final progress = widget.progress;
    
    final elapsedMinSec = _formatDuration(_audioPosition);
    final totalMinSec = _formatDuration(_audioDuration);
    
    final double maxVal = _audioDuration.inMilliseconds.toDouble();
    final double currentVal = _audioPosition.inMilliseconds.toDouble().clamp(0.0, maxVal > 0 ? maxVal : 1.0);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        // File header
        Row(
          children: [
            Icon(Icons.audiotrack_rounded, color: stateColor, size: 20),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                progress.filename,
                style: TextStyle(
                  color: AppTheme.current.text,
                  fontFamily: 'monospace',
                  fontSize: 12,
                  fontWeight: FontWeight.bold,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        
        // Progress Slider
        SliderTheme(
          data: SliderTheme.of(context).copyWith(
            trackHeight: 2,
            thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
            overlayShape: const RoundSliderOverlayShape(overlayRadius: 12),
            activeTrackColor: stateColor,
            inactiveTrackColor: AppTheme.current.text.withValues(alpha: 0.1),
            thumbColor: stateColor,
            overlayColor: stateColor.withValues(alpha: 0.2),
            padding: EdgeInsets.zero,
          ),
          child: Slider(
            value: currentVal,
            min: 0.0,
            max: maxVal > 0 ? maxVal : 1.0,
            onChanged: (value) {
              if (_audioPlayer != null) {
                _audioPlayer!.seek(Duration(milliseconds: value.toInt()));
              }
            },
          ),
        ),
        
        // Controls and Time row
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                // Play/Pause button
                IconButton(
                  icon: Icon(
                    _isPlaying ? Icons.pause_rounded : Icons.play_arrow_rounded,
                    color: stateColor,
                  ),
                  onPressed: _toggleAudioPlayback,
                  constraints: const BoxConstraints(),
                  padding: const EdgeInsets.all(4),
                ),
                const SizedBox(width: 8),
                // Stop button
                IconButton(
                  icon: Icon(
                    Icons.stop_rounded,
                    color: stateColor.withValues(alpha: 0.7),
                  ),
                  onPressed: _stopAudioPlayback,
                  constraints: const BoxConstraints(),
                  padding: const EdgeInsets.all(4),
                ),
              ],
            ),
            Text(
              "$elapsedMinSec / $totalMinSec",
              style: TextStyle(
                color: AppTheme.current.text.withValues(alpha: 0.6),
                fontSize: 10,
                fontFamily: 'monospace',
              ),
            ),
          ],
        ),
      ],
    );
  }

  Future<void> _loadThumbnail() async {
    final progress = widget.progress;
    String? localPath = progress.localPath;
    final String transferId = progress.transferId;
    final String mimeType = progress.mimeType;
    final String ext = progress.filename.split('.').last.toLowerCase();
    final String fileHash = progress.fileHash;

    // Fallback: if localPath is stale, check Sovereign Drive for the organized path
    if (localPath == null || !File(localPath).existsSync()) {
      if (fileHash.isNotEmpty) {
        try {
          final driveInfo = IntrovertClient().driveGetByHash(fileHash);
          if (driveInfo.containsKey('local_path')) {
            final organizedPath = IntrovertClient().resolveSandboxPath(driveInfo['local_path']?.toString()) ?? "";
            if (organizedPath.isNotEmpty && File(organizedPath).existsSync()) {
              localPath = organizedPath;
            }
          }
        } catch (_) {}
      }
    }

    final bool canShowThumbnail = localPath != null &&
        (widget.isMe || progress.isOutgoing || progress.isVerified) &&
        File(localPath).existsSync();

    if (!canShowThumbnail) {
      if (mounted) {
        setState(() {
          _thumbnailPath = null;
          _isGenerating = false;
          _lastLoadedTransferId = transferId;
        });
      }
      return;
    }

    if (_lastLoadedTransferId == transferId && _thumbnailPath != null) {
      return;
    }

    if (_isGenerating) return;

    final bool isImage = mimeType.startsWith('image/') || 
        ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext);
    final bool isVideo = mimeType.startsWith('video/') ||
        ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext);
    final bool isPdf = mimeType.startsWith('application/pdf') ||
        ext == 'pdf';

    if (!isImage && !isVideo && !isPdf) {
      if (mounted) {
        setState(() {
          _thumbnailPath = null;
          _isGenerating = false;
          _lastLoadedTransferId = transferId;
        });
      }
      return;
    }

    if (isImage) {
      // For HEIC/HEIF, try to render — if Image.file() fails, errorBuilder shows placeholder
      // For other formats (PNG, JPG, etc.), display directly
      if (mounted) {
        setState(() {
          _thumbnailPath = localPath;
          _isGenerating = false;
          _lastLoadedTransferId = transferId;
        });
      }
      return;
    }

    setState(() {
      _isGenerating = true;
      _thumbnailPath = null;
      _lastLoadedTransferId = transferId;
    });

    try {
      Directory cacheDir;
      try {
        cacheDir = await getApplicationCacheDirectory();
      } catch (_) {
        cacheDir = await getTemporaryDirectory();
      }
      await cacheDir.create(recursive: true);
      final cachePath = '${cacheDir.path}/thumb_$transferId.png';
      final cacheFile = File(cachePath);

      if (await cacheFile.exists() && await cacheFile.length() > 0) {
        if (mounted) {
          setState(() {
            _thumbnailPath = cachePath;
            _isGenerating = false;
          });
        }
        return;
      }

      if (isVideo) {
        try {
          final result = await FlutterVideoThumbnailPlus.thumbnailFile(
            video: localPath,
            thumbnailPath: cachePath,
            imageFormat: ImageFormat.png,
            maxHeight: 250,
            quality: 75,
          );
          final exists = await File(cachePath).exists();
          if ((result != null || exists) && mounted) {
            setState(() {
              _thumbnailPath = cachePath;
              _isGenerating = false;
            });
          } else if (mounted) {
            setState(() {
              _isGenerating = false;
            });
          }
        } catch (_) {
          if (mounted) setState(() { _isGenerating = false; });
        }
      } else if (isImage && (ext == 'heic' || ext == 'heif')) {
        // HEIC/HEIF: show image icon placeholder (system decoder may not render)
        if (mounted) {
          setState(() {
            _thumbnailPath = null;
            _isGenerating = false;
            _lastLoadedTransferId = transferId;
          });
        }
      } else if (isPdf) {
        try {
          final doc = await PdfDocument.openFile(localPath);
          final page = await doc.getPage(1);
          final pageImage = await page.render(
            width: (page.width * 2).toInt(),
            height: (page.height * 2).toInt(),
          );
          final uiImage = await pageImage.createImageIfNotAvailable();
          final byteData = await uiImage.toByteData(format: ImageByteFormat.png);
          if (byteData != null) {
            await cacheFile.writeAsBytes(byteData.buffer.asUint8List());
          }
          pageImage.dispose();
          await doc.dispose();

          if (await cacheFile.exists() && await cacheFile.length() > 0 && mounted) {
            setState(() {
              _thumbnailPath = cachePath;
              _isGenerating = false;
            });
          } else if (mounted) {
            setState(() { _isGenerating = false; });
          }
        } catch (_) {
          if (mounted) setState(() { _isGenerating = false; });
        }
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          _isGenerating = false;
        });
      }
    }
  }

  String _getCleanStatusText() {
    final progress = widget.progress;
    final bool isComplete = progress.isComplete;
    final bool isVerified = progress.isVerified;
    final bool isGroup = progress.groupId != null && progress.groupId!.isNotEmpty;

    if (progress.isCancelled) {
      return "cancelled";
    }
    if (progress.filename.startsWith("ERROR:")) {
      return "transfer failed";
    }
    if (progress.isWaitingForDownload) {
      return "tap to pull from mesh";
    }
    if (progress.mimeType == 'SWARM_WAIT') {
      return "searching mesh swarm...";
    }

    if (isVerified) {
      if (isGroup && progress.isOutgoing) {
        return "all members verified ✓";
      }
      return "verified ✓";
    }

    // Sender: all chunks pushed but waiting for recipient ACK(s)
    if (progress.isOutgoing && isComplete && !isVerified) {
      if (isGroup) {
        final int pct = (progress.progress * 100).toInt();
        if (pct > 0) {
          return "delivered to $pct% of group · awaiting rest";
        }
        return "waiting for recipients...";
      }
      return "waiting for recipient...";
    }

    if (isComplete && !isVerified) {
      return "verifying...";
    }

    // During transfer: minimal status, no percentage/speed
    final String action = progress.isOutgoing ? "pushing to mesh" : "pulling from mesh";
    return action;
  }

  @override
  Widget build(BuildContext context) {
    final progress = widget.progress;
    final bool isMe = widget.isMe;

    final String ext = progress.filename.split('.').last.toLowerCase();
    final bool isImage = progress.mimeType.startsWith('image/') ||
        ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext);
    final bool isVideo = progress.mimeType.startsWith('video/') ||
        ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext);
    final bool isAudio = progress.mimeType.startsWith('audio/') ||
        ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac'].contains(ext);
    final bool isMedia = isImage || isVideo;
    final bool isComplete = progress.isComplete;
    final bool isVerified = progress.isVerified;
    final String? localPath = progress.localPath;

    // During download: show nothing — image appears when verified
    if (!progress.isVerified && !widget.isMe) {
      return const SizedBox.shrink();
    }

    Color stateColor = isVerified 
        ? AppTheme.current.accent 
        : (progress.isWaitingForDownload ? Colors.orangeAccent : AppTheme.current.accent);
    if (progress.isCancelled) {
      stateColor = Colors.grey;
    } else if (progress.filename.startsWith("ERROR:")) {
      stateColor = Colors.redAccent;
    }

    final cleanStatusText = _getCleanStatusText();

    return Container(
      margin: EdgeInsets.only(
        top: 8, 
        left: 24, 
        right: 24, 
        bottom: (widget.reactions != null && widget.reactions!.isNotEmpty) ? 24 : 8
      ),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          GestureDetector(
            onTap: () {
              if (isVerified && localPath != null) {
                if (isMedia) {
                  // Open custom full screen gallery
                  final List<FileTransferProgress> mediaList = [];
                  if (widget.allMessages != null) {
                    for (var m in widget.allMessages!) {
                      if (m is FileTransferProgress) {
                        final mExt = m.filename.split('.').last.toLowerCase();
                        final mIsImage = m.mimeType.startsWith('image/') || 
                            ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(mExt);
                        final mIsVideo = m.mimeType.startsWith('video/') || 
                            ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(mExt);
                        if ((mIsImage || mIsVideo) && 
                            (m.isVerified || m.isOutgoing) && 
                            m.localPath != null && 
                            File(m.localPath!).existsSync()) {
                          mediaList.add(m);
                        }
                      }
                    }
                  }

                  if (mediaList.isEmpty) {
                    mediaList.add(progress);
                  }

                  int initialIndex = mediaList.indexWhere((m) => m.transferId == progress.transferId);
                  if (initialIndex == -1) initialIndex = 0;

                  Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (context) => MediaGalleryViewer(
                        mediaList: mediaList,
                        initialIndex: initialIndex,
                      ),
                    ),
                  );
                } else {
                  // Non-media file: open with system default viewer
                  debugPrint("📂 Opening file: $localPath");
                  OpenFile.open(localPath);
                }
              } else if (widget.onTap != null) {
                widget.onTap!();
              }
            },
            child: ClipRRect(
              borderRadius: BorderRadius.circular(16),
              child: BackdropFilter(
                filter: ImageFilter.blur(sigmaX: 10, sigmaY: 10),
                child: Container(
                  width: MediaQuery.of(context).size.width * 0.75,
                  decoration: BoxDecoration(
                    color: isMe 
                      ? AppTheme.current.accent.withValues(alpha: isVerified ? 0.15 : 0.1) 
                      : AppTheme.current.text.withValues(alpha: isVerified ? 0.08 : 0.05),
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(
                      color: isVerified ? stateColor : AppTheme.current.text.withValues(alpha: 0.1),
                      width: isVerified ? 1.5 : 1,
                    ),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (isAudio && isVerified && localPath != null && File(localPath).existsSync()) ...[
                        Padding(
                          padding: const EdgeInsets.all(12),
                          child: _buildAudioPlayerWidget(stateColor),
                        ),
                      ] else if (isMedia && ((_thumbnailPath != null && File(_thumbnailPath!).existsSync()) || (progress.thumbnail != null && progress.thumbnail!.isNotEmpty)) && (isVerified || isMe)) ...[
                        // Clean media display: image with rounded corners, no frame
                        ClipRRect(
                          borderRadius: BorderRadius.circular(15),
                          child: _buildThumbnailWidget(stateColor),
                        ),
                        // Caption below image if present
                        if (progress.caption != null && progress.caption!.isNotEmpty)
                          Padding(
                            padding: const EdgeInsets.fromLTRB(12, 4, 12, 10),
                            child: Text(
                              progress.caption!,
                              style: TextStyle(
                                color: AppTheme.current.text.withValues(alpha: 0.9),
                                fontSize: 13,
                                height: 1.3,
                              ),
                            ),
                          ),
                        // Minimal status for media
                        Padding(
                          padding: const EdgeInsets.fromLTRB(12, 0, 12, 10),
                          child: Text(
                            cleanStatusText,
                            style: TextStyle(
                              color: stateColor.withValues(alpha: 0.8),
                              fontSize: 10,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                        ),
                      ] else ...[
                        Padding(
                          padding: const EdgeInsets.all(12),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              _buildThumbnailWidget(stateColor),

                              Row(
                                children: [
                                  if (progress.isWaitingForDownload)
                                    Icon(Icons.download_for_offline_outlined, color: stateColor, size: 20)
                                  else if (!isComplete)
                                    SizedBox(
                                      width: 20,
                                      height: 20,
                                      child: CircularProgressIndicator(
                                        value: progress.progress,
                                        strokeWidth: 1.5,
                                        valueColor: AlwaysStoppedAnimation<Color>(stateColor),
                                      ),
                                    )
                                  else
                                    Icon(
                                      isImage ? Icons.image : (isVideo ? Icons.videocam : Icons.insert_drive_file),
                                      color: stateColor,
                                      size: 20,
                                    ),
                                  const SizedBox(width: 8),
                                  Expanded(
                                    child: Column(
                                      crossAxisAlignment: CrossAxisAlignment.start,
                                      children: [
                                        if (!isMedia) ...[
                                          Text(
                                            progress.filename.replaceFirst("ERROR:", ""),
                                            style: TextStyle(
                                              color: AppTheme.current.text,
                                              fontFamily: 'monospace',
                                              fontSize: 12,
                                              fontWeight: FontWeight.bold,
                                            ),
                                            overflow: TextOverflow.ellipsis,
                                          ),
                                          const SizedBox(height: 2),
                                        ],
                                        Text(
                                          cleanStatusText,
                                          style: TextStyle(
                                            color: stateColor.withValues(alpha: 0.8),
                                            fontSize: 10,
                                            fontWeight: FontWeight.bold,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                  if (isVerified && !isMedia && localPath != null)
                                    TextButton(
                                      onPressed: () => OpenFile.open(localPath),
                                      style: TextButton.styleFrom(
                                        minimumSize: const Size(0, 24),
                                        padding: const EdgeInsets.symmetric(horizontal: 8),
                                      ),
                                      child: Text(
                                        "OPEN",
                                        style: TextStyle(color: stateColor, fontSize: 10, fontWeight: FontWeight.bold),
                                      ),
                                    )
                                  else if (!isComplete && !progress.isCancelled && !progress.isWaitingForDownload)
                                    IconButton(
                                      icon: const Icon(Icons.close_rounded, color: Colors.redAccent, size: 18),
                                      onPressed: () {
                                        try {
                                          IntrovertClient().cancelFileTransfer(progress.transferId);
                                        } catch (e) {
                                          if (context.mounted) {
                                            ScaffoldMessenger.of(context).showSnackBar(
                                              const SnackBar(content: Text("Cancel not yet available in this build")),
                                            );
                                          }
                                        }
                                      },
                                      constraints: const BoxConstraints(),
                                      padding: EdgeInsets.zero,
                                    ),
                                ],
                              ),

                              if (!isComplete && !progress.isWaitingForDownload)
                                Padding(
                                  padding: const EdgeInsets.only(top: 8),
                                  child: ClipRRect(
                                    borderRadius: BorderRadius.circular(2),
                                    child: LinearProgressIndicator(
                                      value: progress.progress,
                                      backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                                      valueColor: AlwaysStoppedAnimation<Color>(stateColor),
                                      minHeight: 2,
                                    ),
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (widget.reactions != null && widget.reactions!.isNotEmpty)
            Positioned(
              bottom: -4,
              left: isMe ? null : 8,
              right: isMe ? 8 : null,
              child: _buildReactionsRow(widget.reactions!),
            ),
        ],
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
        boxShadow: [BoxShadow(color: Colors.black.withValues(alpha: 0.3), blurRadius: 4, offset: const Offset(0, 2))],
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
                Text("${e.value}", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 9, fontWeight: FontWeight.bold)),
              ],
            ],
          ),
        )).toList(),
      ),
    );
  }

  Widget _buildThumbnailWidget(Color stateColor) {
    final progress = widget.progress;

    if (_thumbnailPath != null) {
      final file = File(_thumbnailPath!);
      if (file.existsSync()) {
        final String ext = progress.filename.split('.').last.toLowerCase();
        final bool isVideo = progress.mimeType.startsWith('video/') ||
            ['mp4', 'mkv', 'mov', 'avi', 'webm'].contains(ext);
        final bool isPdf = progress.mimeType.startsWith('application/pdf') ||
            ext == 'pdf';

        return Padding(
          padding: const EdgeInsets.only(bottom: 12),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: Container(
              color: Colors.black12,
              constraints: const BoxConstraints(
                maxHeight: 330,
                minHeight: 180,
                minWidth: double.infinity,
              ),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  Image.file(
                    file,
                    fit: BoxFit.contain,
                    errorBuilder: (context, error, stackTrace) => _buildPlaceholder(
                      progress.filename,
                      progress.mimeType,
                      stateColor,
                    ),
                  ),
                  if (isVideo)
                    Container(
                      decoration: BoxDecoration(
                        color: Colors.black.withValues(alpha: 0.4),
                        shape: BoxShape.circle,
                      ),
                      padding: const EdgeInsets.all(12),
                      child: Icon(
                        Icons.play_arrow_rounded,
                        color: AppTheme.current.text,
                        size: 36,
                      ),
                    ),
                  if (isPdf)
                    Positioned(
                      top: 8,
                      left: 8,
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                        decoration: BoxDecoration(
                          color: Colors.redAccent.withValues(alpha: 0.85),
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: Text(
                          "PDF",
                          style: TextStyle(
                            color: AppTheme.current.text,
                            fontSize: 9,
                            fontWeight: FontWeight.bold,
                            fontFamily: 'monospace',
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        );
      }
    } else if (progress.thumbnail != null && progress.thumbnail!.isNotEmpty) {
      try {
        final bytes = base64Decode(progress.thumbnail!);
        final String ext = progress.filename.split('.').last.toLowerCase();
        final bool isVideo = progress.mimeType.startsWith('video/') ||
            ['mp4', 'mkv', 'mov', 'avi', 'webm'].contains(ext);
        final bool isPdf = progress.mimeType.startsWith('application/pdf') ||
            ext == 'pdf';

        return Padding(
          padding: const EdgeInsets.only(bottom: 12),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: Container(
              color: Colors.black12,
              constraints: const BoxConstraints(
                maxHeight: 330,
                minHeight: 180,
                minWidth: double.infinity,
              ),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  Image.memory(
                    bytes,
                    fit: BoxFit.contain,
                    errorBuilder: (context, error, stackTrace) => _buildPlaceholder(
                      progress.filename,
                      progress.mimeType,
                      stateColor,
                    ),
                  ),
                  if (isVideo)
                    Container(
                      decoration: BoxDecoration(
                        color: Colors.black.withValues(alpha: 0.4),
                        shape: BoxShape.circle,
                      ),
                      padding: const EdgeInsets.all(12),
                      child: Icon(
                        Icons.play_arrow_rounded,
                        color: AppTheme.current.text,
                        size: 36,
                      ),
                    ),
                  if (isPdf)
                    Positioned(
                      top: 8,
                      left: 8,
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                        decoration: BoxDecoration(
                          color: Colors.redAccent.withValues(alpha: 0.85),
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: Text(
                          "PDF",
                          style: TextStyle(
                            color: AppTheme.current.text,
                            fontSize: 9,
                            fontWeight: FontWeight.bold,
                            fontFamily: 'monospace',
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        );
      } catch (e) {
        debugPrint("Error rendering base64 thumbnail: $e");
      }
    }

    if (_isGenerating) {
      return Padding(
        padding: const EdgeInsets.only(bottom: 12),
        child: Stack(
          children: [
            _buildPlaceholder(progress.filename, progress.mimeType, stateColor),
            Positioned.fill(
              child: Center(
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  valueColor: AlwaysStoppedAnimation<Color>(AppTheme.current.text.withValues(alpha: 0.3)),
                ),
              ),
            ),
          ],
        ),
      );
    }

    if (progress.isWaitingForDownload) {
      return _buildPlaceholder(progress.filename, progress.mimeType, stateColor);
    } else if (!progress.isVerified && !widget.isMe) {
      // Download happening silently in background — nothing shown until verified
      return const SizedBox.shrink();
    } else {
      return _buildPlaceholder(progress.filename, progress.mimeType, stateColor);
    }
  }

  Widget _buildPlaceholder(String filename, String mimeType, Color color) {
    final String ext = filename.split('.').last.toUpperCase();
    final bool isPdf = ext == 'PDF';
    final bool isDoc = ['DOC', 'DOCX'].contains(ext);
    final bool isSheet = ['XLS', 'XLSX', 'CSV'].contains(ext);
    final bool isZip = ['ZIP', 'RAR', '7Z', 'TAR', 'GZ'].contains(ext);
    final bool isImage = mimeType.startsWith('image/') || ['HEIC', 'HEIF'].contains(ext);
    final bool isVideo = mimeType.startsWith('video/');
    
    Color boxColor = AppTheme.current.text.withValues(alpha: 0.05);
    if (isPdf) {
      boxColor = Colors.redAccent.withValues(alpha: 0.15);
    } else if (isDoc) {
      boxColor = Colors.blueAccent.withValues(alpha: 0.15);
    } else if (isSheet) {
      boxColor = AppTheme.current.accent.withValues(alpha: 0.15);
    } else if (isZip) {
      boxColor = Colors.orangeAccent.withValues(alpha: 0.15);
    }

    IconData icon = isImage ? Icons.image : (isVideo ? Icons.videocam : Icons.insert_drive_file);

    return Container(
      height: 100,
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 12),
      decoration: BoxDecoration(
        color: boxColor,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: boxColor.withValues(alpha: 0.3), width: 1),
      ),
      child: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              ".$ext",
              style: TextStyle(
                color: AppTheme.current.text.withValues(alpha: 0.7),
                fontSize: 22,
                fontWeight: FontWeight.w900,
                fontFamily: 'monospace',
                letterSpacing: 2,
              ),
            ),
            const SizedBox(height: 6),
            Icon(icon, color: AppTheme.current.mutedText.withValues(alpha: 0.1), size: 20),
          ],
        ),
      ),
    );
  }

  Widget _buildIncomingPlaceholder(String filename, String mimeType, Color color) {
    return Stack(
      children: [
        _buildPlaceholder(filename, mimeType, color),
        Positioned.fill(
          child: Center(
            child: CircularProgressIndicator(
              strokeWidth: 1, 
              valueColor: AlwaysStoppedAnimation<Color>(color.withValues(alpha: 0.2))
            ),
          ),
        ),
      ],
    );
  }
}
