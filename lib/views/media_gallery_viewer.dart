import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
// ignore: implementation_imports
import 'package:vector_math/vector_math_64.dart' show Vector3, Quaternion;
import 'package:open_file/open_file.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
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

class _MediaGalleryViewerState extends State<MediaGalleryViewer> with TickerProviderStateMixin {
  late PageController _pageController;
  late int _currentIndex;
  bool _showControls = true;

  // Swipe-to-dismiss state
  double _dismissDragY = 0.0;
  bool _isDismissing = false;
  bool _isZoomed = false;

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex;
    _pageController = PageController(initialPage: widget.initialIndex);
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
  }

  @override
  void dispose() {
    _pageController.dispose();
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    super.dispose();
  }

  void _toggleControls() {
    setState(() => _showControls = !_showControls);
  }

  void _onVerticalDragUpdate(DragUpdateDetails details) {
    // Block dismiss gesture when zoomed in — let InteractiveViewer handle panning
    if (_isZoomed) return;
    setState(() {
      _dismissDragY += details.delta.dy;
      _isDismissing = _dismissDragY.abs() > 50;
    });
  }

  void _onVerticalDragEnd(DragEndDetails details) {
    if (_dismissDragY.abs() > 120 || details.velocity.pixelsPerSecond.dy.abs() > 500) {
      Navigator.of(context).pop();
    } else {
      setState(() {
        _dismissDragY = 0;
        _isDismissing = false;
      });
    }
  }

  Future<void> _shareCurrentMedia() async {
    final item = widget.mediaList[_currentIndex];
    if (item.localPath == null) return;
    try {
      await Share.shareXFiles([XFile(item.localPath!)], text: item.filename);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Share failed: $e'), backgroundColor: Colors.redAccent),
        );
      }
    }
  }

  Future<void> _saveToDownloads() async {
    final item = widget.mediaList[_currentIndex];
    if (item.localPath == null) return;
    try {
      final src = File(item.localPath!);
      if (!src.existsSync()) return;

      // On mobile, save to the app's external documents dir (accessible via file manager)
      // On desktop, save to Downloads
      Directory saveDir;
      if (Platform.isAndroid || Platform.isIOS) {
        saveDir = await getApplicationDocumentsDirectory();
      } else {
        final home = Platform.environment['HOME'] ?? '/tmp';
        saveDir = Directory('$home/Downloads');
        if (!saveDir.existsSync()) saveDir.createSync(recursive: true);
      }

      final destPath = '${saveDir.path}/${item.filename}';
      await src.copy(destPath);

      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Saved to ${Platform.isAndroid || Platform.isIOS ? "Documents" : "Downloads"}'),
            backgroundColor: Colors.green.shade700,
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Save failed: $e'), backgroundColor: Colors.redAccent),
        );
      }
    }
  }

  void _openExternally() {
    final item = widget.mediaList[_currentIndex];
    if (item.localPath != null) {
      OpenFile.open(item.localPath!);
    }
  }

  @override
  Widget build(BuildContext context) {
    final scale = 1.0 - (_dismissDragY.abs() / 600).clamp(0.0, 0.3);
    final opacity = 1.0 - (_dismissDragY.abs() / 400).clamp(0.0, 1.0);

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: GestureDetector(
        onVerticalDragUpdate: _onVerticalDragUpdate,
        onVerticalDragEnd: _onVerticalDragEnd,
        child: AnimatedContainer(
          duration: _isDismissing ? Duration.zero : const Duration(milliseconds: 200),
          transform: Matrix4.translationValues(0, _dismissDragY, 0)..scaleByDouble(scale, scale, scale, 1),
          child: Opacity(
            opacity: opacity,
            child: Stack(
              children: [
                // Content Viewer
                PageView.builder(
                  controller: _pageController,
                  physics: _isZoomed
                    ? const NeverScrollableScrollPhysics()
                    : const BouncingScrollPhysics(),
                  itemCount: widget.mediaList.length,
                  onPageChanged: (index) {
                    setState(() => _currentIndex = index);
                  },
                  itemBuilder: (context, index) {
                    final item = widget.mediaList[index];
                    final ext = item.filename.split('.').last.toLowerCase();
                    final isVideo = item.mimeType.startsWith('video/') ||
                        ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext);

                    if (item.localPath == null || !File(item.localPath!).existsSync()) {
                      return const Center(
                        child: Text("File missing on disk",
                            style: TextStyle(color: Colors.redAccent, fontSize: 14)),
                      );
                    }

                    if (isVideo) {
                      return Center(
                        child: VideoPlayerWidget(
                          localPath: item.localPath!,
                          showControls: _showControls,
                          bottomPadding: 100.0,
                        ),
                      );
                    }

                    return _ZoomableImage(
                      imagePath: item.localPath!,
                      onTap: _toggleControls,
                      onScaleChanged: (scale) {
                        final zoomed = scale > 1.05;
                        if (zoomed != _isZoomed) {
                          setState(() => _isZoomed = zoomed);
                        }
                      },
                    );
                  },
                ),

                // Header
                if (_showControls)
                  Positioned(
                    top: 0,
                    left: 0,
                    right: 0,
                    child: _buildHeader(),
                  ),

                // Bottom action bar
                if (_showControls)
                  Positioned(
                    bottom: 0,
                    left: 0,
                    right: 0,
                    child: _buildBottomBar(),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildHeader() {
    return Container(
      padding: EdgeInsets.only(
        top: MediaQuery.of(context).padding.top + 8,
        bottom: 16,
        left: 8,
        right: 16,
      ),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [Colors.black.withValues(alpha: 0.7), Colors.transparent],
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
        ),
      ),
      child: Row(
        children: [
          IconButton(
            icon: const Icon(Icons.arrow_back_ios_new_rounded, color: Colors.white, size: 22),
            onPressed: () => Navigator.of(context).pop(),
          ),
          const SizedBox(width: 4),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  widget.mediaList[_currentIndex].filename,
                  style: const TextStyle(
                    color: Colors.white,
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                if (widget.mediaList.length > 1)
                  Text(
                    "${_currentIndex + 1} / ${widget.mediaList.length}",
                    style: TextStyle(color: Colors.white.withValues(alpha: 0.5), fontSize: 11),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildBottomBar() {
    return Container(
      padding: EdgeInsets.only(
        bottom: MediaQuery.of(context).padding.bottom + 12,
        top: 12,
        left: 16,
        right: 16,
      ),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [Colors.transparent, Colors.black.withValues(alpha: 0.7)],
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
        ),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceEvenly,
        children: [
          _buildActionButton(Icons.reply_rounded, "Forward", () {
            // Forward/share
            _shareCurrentMedia();
          }),
          _buildActionButton(Icons.download_rounded, "Save", _saveToDownloads),
          _buildActionButton(Icons.share_rounded, "Share", _shareCurrentMedia),
          _buildActionButton(Icons.open_in_new_rounded, "Open", _openExternally),
        ],
      ),
    );
  }

  Widget _buildActionButton(IconData icon, String label, VoidCallback onTap) {
    return GestureDetector(
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: Colors.white.withValues(alpha: 0.15),
              shape: BoxShape.circle,
            ),
            child: Icon(icon, color: Colors.white, size: 22),
          ),
          const SizedBox(height: 4),
          Text(
            label,
            style: TextStyle(color: Colors.white.withValues(alpha: 0.8), fontSize: 10),
          ),
        ],
      ),
    );
  }
}

/// A single zoomable image widget with double-tap-to-zoom and pinch-to-zoom.
/// Uses a TransformationController for precise control over zoom state.
class _ZoomableImage extends StatefulWidget {
  final String imagePath;
  final VoidCallback onTap;
  final void Function(double scale)? onScaleChanged;

  const _ZoomableImage({
    required this.imagePath,
    required this.onTap,
    this.onScaleChanged,
  });

  @override
  State<_ZoomableImage> createState() => _ZoomableImageState();
}

class _ZoomableImageState extends State<_ZoomableImage> with SingleTickerProviderStateMixin {
  final TransformationController _controller = TransformationController();
  late AnimationController _animController;
  Animation<Matrix4>? _animation;

  // Track current scale to route gestures
  double _currentScale = 1.0;

  // Double-tap zoom levels
  static const double _minScale = 1.0;
  static const double _maxScale = 5.0;
  static const double _doubleTapScale = 2.5;

  @override
  void initState() {
    super.initState();
    _animController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 250),
    )..addListener(() {
        if (_animation != null) {
          _controller.value = _animation!.value;
        }
      });
  }

  @override
  void dispose() {
    _controller.dispose();
    _animController.dispose();
    super.dispose();
  }

  void _onDoubleTap(TapDownDetails details) {
    final tapPosition = details.localPosition;

    if (_currentScale > 1.1) {
      // Already zoomed — zoom back to 1x
      _animateToScale(Matrix4.identity(), 1.0);
    } else {
      // Zoom to doubleTapScale centered on tap position
      final translated = Matrix4.compose(
        Vector3(-tapPosition.dx * (_doubleTapScale - 1), -tapPosition.dy * (_doubleTapScale - 1), 0),
        Quaternion.identity(),
        Vector3.all(_doubleTapScale),
      );
      _animateToScale(translated, _doubleTapScale);
    }
  }

  void _animateToScale(Matrix4 target, double newScale) {
    _animation = Matrix4Tween(begin: _controller.value, end: target)
        .animate(CurvedAnimation(parent: _animController, curve: Curves.easeOutCubic));
    _animController.forward(from: 0);
    setState(() => _currentScale = newScale);
    widget.onScaleChanged?.call(newScale);
  }

  void _onInteractionStart(ScaleStartDetails details) {
    // Stop any running animation when user starts interacting
    if (_animController.isAnimating) {
      _animController.stop();
    }
  }

  void _onInteractionUpdate(ScaleUpdateDetails details) {
    // Track the current scale from the transformation matrix
    final scale = _controller.value.getMaxScaleOnAxis();
    if (scale != _currentScale) {
      setState(() => _currentScale = scale);
      widget.onScaleChanged?.call(scale);
    }
  }

  void _onInteractionEnd(ScaleEndDetails details) {
    final scale = _controller.value.getMaxScaleOnAxis();

    // Snap back to 1x if slightly zoomed out
    if (scale < 1.0) {
      _animateToScale(Matrix4.identity(), 1.0);
    }
    // Clamp if over-zoomed
    else if (scale > _maxScale) {
      final center = MediaQuery.of(context).size.center(Offset.zero);
      final clamped = Matrix4.identity()
        ..translateByDouble(-center.dx * (_maxScale - 1), -center.dy * (_maxScale - 1), 0, 1)
        ..scaleByDouble(_maxScale, _maxScale, _maxScale, 1);
      _animateToScale(clamped, _maxScale);
    }
  }

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: widget.onTap,
      onDoubleTapDown: _onDoubleTap,
      child: InteractiveViewer(
        transformationController: _controller,
        minScale: _minScale,
        maxScale: _maxScale,
        onInteractionStart: _onInteractionStart,
        onInteractionUpdate: _onInteractionUpdate,
        onInteractionEnd: _onInteractionEnd,
        // Always allow pan; PageView NeverScrollableScrollPhysics handles the conflict when zoomed
        panEnabled: true,
        scaleEnabled: true,
        child: Center(
          child: Image.file(
            File(widget.imagePath),
            fit: BoxFit.contain,
            errorBuilder: (context, error, stackTrace) => const Center(
              child: Icon(Icons.broken_image, color: Colors.grey, size: 64),
            ),
          ),
        ),
      ),
    );
  }
}



// ============================================================================
// Video Player Widget (unchanged from original)
// ============================================================================

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
      return const Center(child: CircularProgressIndicator(color: Colors.white));
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
