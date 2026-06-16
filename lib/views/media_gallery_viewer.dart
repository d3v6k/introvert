import 'package:flutter/material.dart';
import 'dart:io';
import 'package:video_player/video_player.dart';
import '../src/native/introvert_client.dart';
import '../theme/app_theme.dart';

class MediaGalleryViewer extends StatefulWidget {
  final List<FileTransferProgress> mediaList;
  final int initialIndex;

  const MediaGalleryViewer({
    required this.mediaList,
    required this.initialIndex,
    super.key,
  });

  @override
  State<MediaGalleryViewer> createState() => _MediaGalleryViewerState();
}

class _MediaGalleryViewerState extends State<MediaGalleryViewer> {
  late PageController _pageController;
  late ScrollController _dotScrollController;
  late int _currentIndex;
  bool _showControls = true;

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex;
    _pageController = PageController(initialPage: widget.initialIndex);
    _dotScrollController = ScrollController();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _scrollToActiveDot(widget.initialIndex, animate: false);
      }
    });
  }

  @override
  void dispose() {
    _pageController.dispose();
    _dotScrollController.dispose();
    super.dispose();
  }

  void _scrollToActiveDot(int index, {bool animate = true}) {
    if (!_dotScrollController.hasClients) return;

    final double dotCenter = index * 16.0 + 16.0;
    final double viewportWidth = MediaQuery.of(context).size.width - 32;
    final double targetOffset = dotCenter - (viewportWidth / 2);

    final double maxScroll = _dotScrollController.position.maxScrollExtent;
    final double minScroll = _dotScrollController.position.minScrollExtent;
    final double clampedOffset = targetOffset.clamp(minScroll, maxScroll);

    if (animate) {
      _dotScrollController.animateTo(
        clampedOffset,
        duration: const Duration(milliseconds: 200),
        curve: Curves.easeInOut,
      );
    } else {
      _dotScrollController.jumpTo(clampedOffset);
    }
  }

  void _toggleControls() {
    setState(() {
      _showControls = !_showControls;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black,
      body: Stack(
        children: [
          // Content Viewer
          GestureDetector(
            onTap: _toggleControls,
            child: PageView.builder(
              controller: _pageController,
              itemCount: widget.mediaList.length,
              onPageChanged: (index) {
                setState(() {
                  _currentIndex = index;
                });
                _scrollToActiveDot(index);
              },
              itemBuilder: (context, index) {
                final item = widget.mediaList[index];
                final ext = item.filename.split('.').last.toLowerCase();
                final isVideo = item.mimeType.startsWith('video/') ||
                    ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext);

                if (item.localPath == null || !File(item.localPath!).existsSync()) {
                  return const Center(
                    child: Text(
                      "File missing on disk",
                      style: TextStyle(color: Colors.redAccent, fontSize: 14),
                    ),
                  );
                }

                if (isVideo) {
                  return Center(
                    child: VideoPlayerWidget(
                      localPath: item.localPath!,
                      showControls: _showControls,
                      bottomPadding: widget.mediaList.length > 1 ? 80.0 : 24.0,
                    ),
                  );
                } else {
                  // Image viewer with pinch to zoom
                  return InteractiveViewer(
                    minScale: 0.5,
                    maxScale: 4.0,
                    child: Center(
                      child: Image.file(
                        File(item.localPath!),
                        fit: BoxFit.contain,
                        errorBuilder: (context, error, stackTrace) => const Center(
                          child: Icon(Icons.broken_image, color: Colors.grey, size: 64),
                        ),
                      ),
                    ),
                  );
                }
              },
            ),
          ),

          // Header Overlay
          if (_showControls)
            Positioned(
              top: 0,
              left: 0,
              right: 0,
              child: Container(
                padding: EdgeInsets.only(
                  top: MediaQuery.of(context).padding.top + 8,
                  bottom: 16,
                  left: 16,
                  right: 16,
                ),
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    colors: [Colors.black.withValues(alpha: 0.8), Colors.transparent],
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                  ),
                ),
                child: Row(
                  children: [
                    IconButton(
                      icon: const Icon(Icons.arrow_back_ios_new_rounded, color: Colors.white),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            widget.mediaList[_currentIndex].filename,
                            style: const TextStyle(
                              color: Colors.white,
                              fontSize: 14,
                              fontWeight: FontWeight.bold,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            "${_currentIndex + 1} of ${widget.mediaList.length}",
                            style: TextStyle(
                              color: Colors.white.withValues(alpha: 0.6),
                              fontSize: 11,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),

          // Bottom Roll/Thumbnail Strip Indicator
          if (_showControls && widget.mediaList.length > 1)
            Positioned(
              bottom: MediaQuery.of(context).padding.bottom + 16,
              left: 16,
              right: 16,
              child: Center(
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: MediaQuery.of(context).size.width - 32,
                  ),
                  child: Container(
                    height: 48,
                    decoration: BoxDecoration(
                      color: Colors.black.withValues(alpha: 0.6),
                      borderRadius: BorderRadius.circular(24),
                      border: Border.all(color: Colors.white12),
                    ),
                    child: SingleChildScrollView(
                      controller: _dotScrollController,
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: List.generate(widget.mediaList.length, (index) {
                          final isSelected = index == _currentIndex;
                          return GestureDetector(
                            onTap: () {
                              _pageController.animateToPage(
                                index,
                                duration: const Duration(milliseconds: 300),
                                curve: Curves.easeInOut,
                              );
                            },
                            child: AnimatedContainer(
                              duration: const Duration(milliseconds: 200),
                              margin: const EdgeInsets.symmetric(horizontal: 4),
                              width: isSelected ? 24 : 8,
                              height: 8,
                              decoration: BoxDecoration(
                                color: isSelected ? AppTheme.current.accent : Colors.white38,
                                borderRadius: BorderRadius.circular(4),
                              ),
                            ),
                          );
                        }),
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class VideoPlayerWidget extends StatefulWidget {
  final String localPath;
  final bool showControls;
  final double bottomPadding;

  const VideoPlayerWidget({
    required this.localPath,
    required this.showControls,
    this.bottomPadding = 24.0,
    super.key,
  });

  @override
  State<VideoPlayerWidget> createState() => _VideoPlayerWidgetState();
}

class _VideoPlayerWidgetState extends State<VideoPlayerWidget> {
  late VideoPlayerController _controller;
  bool _initialized = false;
  bool _isPlaying = false;
  Duration _position = Duration.zero;
  Duration _duration = Duration.zero;

  @override
  void initState() {
    super.initState();
    _controller = VideoPlayerController.file(File(widget.localPath))
      ..initialize().then((_) {
        if (mounted) {
          setState(() {
            _initialized = true;
            _duration = _controller.value.duration;
          });
        }
      });

    _controller.addListener(() {
      if (mounted) {
        setState(() {
          _position = _controller.value.position;
          _isPlaying = _controller.value.isPlaying;
        });
      }
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _togglePlay() {
    setState(() {
      if (_controller.value.isPlaying) {
        _controller.pause();
      } else {
        _controller.play();
      }
    });
  }

  String _formatDuration(Duration d) {
    final minutes = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final seconds = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return "$minutes:$seconds";
  }

  @override
  Widget build(BuildContext context) {
    if (!_initialized) {
      return const Center(
        child: CircularProgressIndicator(color: Colors.white),
      );
    }

    return Stack(
      fit: StackFit.expand,
      children: [
        Center(
          child: AspectRatio(
            aspectRatio: _controller.value.aspectRatio,
            child: VideoPlayer(_controller),
          ),
        ),
        
        // Big Center Play/Pause Indicator Overlay
        if (widget.showControls)
          Center(
            child: GestureDetector(
              onTap: _togglePlay,
              child: Container(
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: Colors.black45,
                  shape: BoxShape.circle,
                  border: Border.all(color: Colors.white24),
                ),
                child: Icon(
                  _isPlaying ? Icons.pause_rounded : Icons.play_arrow_rounded,
                  color: Colors.white,
                  size: 48,
                ),
              ),
            ),
          ),

        // Bottom Controls Progress Slider Overlay
        if (widget.showControls)
          Positioned(
            bottom: widget.bottomPadding,
            left: 0,
            right: 0,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  colors: [Colors.transparent, Colors.black.withValues(alpha: 0.8)],
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                ),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  // Slider
                  SliderTheme(
                    data: SliderTheme.of(context).copyWith(
                      trackHeight: 3,
                      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
                      overlayShape: const RoundSliderOverlayShape(overlayRadius: 12),
                      activeTrackColor: AppTheme.current.accent,
                      inactiveTrackColor: Colors.white24,
                      thumbColor: AppTheme.current.accent,
                    ),
                    child: Slider(
                      value: _position.inMilliseconds.toDouble().clamp(
                        0.0,
                        _duration.inMilliseconds.toDouble() > 0
                            ? _duration.inMilliseconds.toDouble()
                            : 1.0,
                      ),
                      min: 0.0,
                      max: _duration.inMilliseconds.toDouble() > 0
                          ? _duration.inMilliseconds.toDouble()
                          : 1.0,
                      onChanged: (value) {
                        _controller.seekTo(Duration(milliseconds: value.toInt()));
                      },
                    ),
                  ),
                  
                  // Time and Actions Row
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(
                        "${_formatDuration(_position)} / ${_formatDuration(_duration)}",
                        style: const TextStyle(color: Colors.white, fontSize: 11),
                      ),
                      IconButton(
                        icon: Icon(
                          _controller.value.volume > 0
                              ? Icons.volume_up_rounded
                              : Icons.volume_off_rounded,
                          color: Colors.white,
                          size: 20,
                        ),
                        onPressed: () {
                          setState(() {
                            _controller.setVolume(_controller.value.volume > 0 ? 0.0 : 1.0);
                          });
                        },
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
      ],
    );
  }
}
