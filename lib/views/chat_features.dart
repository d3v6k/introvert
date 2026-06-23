import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:audioplayers/audioplayers.dart';
import 'package:image_picker/image_picker.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:flutter_map/flutter_map.dart';
import 'package:latlong2/latlong.dart';
import '../theme/app_theme.dart';

// Helper to build a consistent reaction row for all bubbles
Widget _buildReactionsRow(List<dynamic> reactions) {
  final Map<String, int> counts = {};
  for (var r in reactions) {
     final emoji = r['emoji']?.toString() ?? '';
     if (emoji.isNotEmpty) counts[emoji] = (counts[emoji] ?? 0) + 1;
  }

  return Container(
    padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
    decoration: BoxDecoration(
      color: const Color(0xFF1E2430),
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
  );
}

// ==========================================
// 1. STICKER BUBBLE WIDGET
// ==========================================
class StickerBubble extends StatelessWidget {
  final String name;
  final bool isMe;
  final DateTime timestamp;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const StickerBubble({
    required this.name,
    required this.isMe,
    required this.timestamp,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final bool isFile = name.startsWith('/') || name.contains('/') || name.endsWith('.png');
    final File file = isFile ? File(name) : File('');
    final bool exists = isFile && file.existsSync();

    return Container(
      margin: EdgeInsets.only(top: 6, left: 16, right: 16, bottom: (reactions != null && reactions!.isNotEmpty) ? 20 : 6),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Column(
            crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
            children: [
              Container(
                padding: EdgeInsets.all(4),
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(16),
                  color: Colors.transparent,
                ),
                child: isFile
                    ? (exists
                        ? Image.file(
                            file,
                            width: 140,
                            height: 140,
                            fit: BoxFit.contain,
                            errorBuilder: (context, error, stackTrace) => _buildErrorPlaceholder(),
                          )
                        : Container(
                            width: 140,
                            height: 140,
                            alignment: Alignment.center,
                            decoration: BoxDecoration(
                              color: AppTheme.current.mutedText.withValues(alpha: 0.1),
                              borderRadius: BorderRadius.circular(12),
                            ),
                            child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent),
                          ))
                    : Image.asset(
                        'assets/images/stickers/$name.png',
                        width: 140,
                        height: 140,
                        fit: BoxFit.contain,
                        errorBuilder: (context, error, stackTrace) => _buildErrorPlaceholder(),
                      ),
              ),
              SizedBox(height: 4),
              Text(
                "${timestamp.hour.toString().padLeft(2, '0')}:${timestamp.minute.toString().padLeft(2, '0')}",
                style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 8, fontFamily: 'monospace'),
              ),
            ],
          ),
          if (reactions != null && reactions!.isNotEmpty)
            Positioned(
              bottom: -4,
              left: isMe ? null : 8,
              right: isMe ? 8 : null,
              child: _buildReactionsRow(reactions!),
            ),
        ],
      ),
    );
  }

  Widget _buildErrorPlaceholder() {
    return Container(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: AppTheme.current.mutedText.withValues(alpha: 0.1),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Text(
        "Sticker: ${name.split('/').last}",
        style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontFamily: 'monospace'),
      ),
    );
  }
}

// ==========================================
// 2. GIF BUBBLE WIDGET
// ==========================================
class GifBubble extends StatelessWidget {
  final String url;
  final bool isMe;
  final DateTime timestamp;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const GifBubble({
    required this.url,
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
      margin: EdgeInsets.only(top: 6, left: 16, right: 16, bottom: (reactions != null && reactions!.isNotEmpty) ? 20 : 6),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Column(
            crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
            children: [
              ClipRRect(
                borderRadius: BorderRadius.circular(16),
                child: Container(
                  color: AppTheme.current.text.withValues(alpha: 0.05),
                  child: Image.network(
                    url,
                    width: 200,
                    height: 150,
                    fit: BoxFit.cover,
                    errorBuilder: (context, error, stackTrace) {
                      return Container(
                        width: 200,
                        height: 150,
                        alignment: Alignment.center,
                        child: Text("GIF Load Error", style: TextStyle(color: Colors.redAccent)),
                      );
                    },
                  ),
                ),
              ),
              SizedBox(height: 4),
              Text(
                "${timestamp.hour.toString().padLeft(2, '0')}:${timestamp.minute.toString().padLeft(2, '0')}",
                style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 8, fontFamily: 'monospace'),
              ),
            ],
          ),
          if (reactions != null && reactions!.isNotEmpty)
            Positioned(
              bottom: -4,
              left: isMe ? null : 8,
              right: isMe ? 8 : null,
              child: _buildReactionsRow(reactions!),
            ),
        ],
      ),
    );
  }
}

// ==========================================
// 3. INTERACTIVE VOICE MEMO BUBBLE
// ==========================================
class VoiceMemoBubble extends StatefulWidget {
  final String filename;
  final bool isMe;
  final DateTime timestamp;
  final String localPath;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const VoiceMemoBubble({
    required this.filename,
    required this.isMe,
    required this.timestamp,
    required this.localPath,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    super.key,
  });

  @override
  State<VoiceMemoBubble> createState() => _VoiceMemoBubbleState();
}

class _VoiceMemoBubbleState extends State<VoiceMemoBubble> {
  final AudioPlayer _audioPlayer = AudioPlayer();
  bool _isPlaying = false;
  double _progress = 0.0;
  int _durationSeconds = 10;
  StreamSubscription? _positionSubscription;
  StreamSubscription? _playerStateSubscription;
  StreamSubscription? _completeSubscription;

  final List<double> _waveformHeights = [
    0.3, 0.5, 0.8, 0.4, 0.6, 0.7, 0.9, 0.3, 0.5, 0.7, 0.4, 0.8, 0.6, 0.9, 0.5, 0.3, 0.7, 0.8, 0.4, 0.6
  ];

  @override
  void initState() {
    super.initState();
    final match = RegExp(r'voice_memo_(\d+)s\.m4a').firstMatch(widget.filename);
    if (match != null) {
      _durationSeconds = int.tryParse(match.group(1) ?? '10') ?? 10;
    }
    _initAudioPlayer();
  }

  void _initAudioPlayer() {
    _positionSubscription = _audioPlayer.onPositionChanged.listen((pos) {
      if (mounted && _audioPlayer.state == PlayerState.playing) {
        setState(() {
          final totalMs = _durationSeconds * 1000;
          if (totalMs > 0) {
            _progress = pos.inMilliseconds / totalMs;
            if (_progress > 1.0) _progress = 1.0;
          }
        });
      }
    });

    _playerStateSubscription = _audioPlayer.onPlayerStateChanged.listen((state) {
      if (mounted) {
        setState(() {
          _isPlaying = state == PlayerState.playing;
        });
      }
    });

    _completeSubscription = _audioPlayer.onPlayerComplete.listen((_) {
      if (mounted) {
        setState(() {
          _progress = 0.0;
          _isPlaying = false;
        });
      }
    });
  }

  @override
  void dispose() {
    _positionSubscription?.cancel();
    _playerStateSubscription?.cancel();
    _completeSubscription?.cancel();
    _audioPlayer.dispose();
    super.dispose();
  }

  void _togglePlayback() async {
    if (_isPlaying) {
      await _audioPlayer.pause();
    } else {
      if (widget.localPath.isNotEmpty && File(widget.localPath).existsSync()) {
        try {
          await _audioPlayer.play(DeviceFileSource(widget.localPath));
        } catch (e) {
          debugPrint("Error playing audio: $e");
        }
      } else {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Audio file not found locally")),
        );
      }
    }
  }

  String _formatDuration(double progress) {
    final elapsed = (progress * _durationSeconds).toInt();
    final mins = (elapsed ~/ 60).toString();
    final secs = (elapsed % 60).toString().padLeft(2, '0');
    return "$mins:$secs";
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: EdgeInsets.only(top: 6, left: 12, right: 12, bottom: (widget.reactions != null && widget.reactions!.isNotEmpty) ? 20 : 6),
      alignment: widget.isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Container(
            constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
            padding: EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              color: widget.isMe
                  ? AppTheme.current.accent.withValues(alpha: 0.15)
                  : AppTheme.current.text.withValues(alpha: 0.08),
              borderRadius: BorderRadius.only(
                topLeft: const Radius.circular(16),
                topRight: const Radius.circular(16),
                bottomLeft: Radius.circular(widget.isMe ? 16 : 4),
                bottomRight: Radius.circular(widget.isMe ? 4 : 16),
              ),
              border: Border.all(
                color: widget.isMe
                    ? AppTheme.current.accent.withValues(alpha: 0.1)
                    : AppTheme.current.text.withValues(alpha: 0.05),
                width: 0.5,
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    GestureDetector(
                      onTap: _togglePlayback,
                      child: Container(
                        width: 36,
                        height: 36,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: AppTheme.current.accent.withValues(alpha: 0.2),
                        ),
                        child: Icon(
                          _isPlaying ? Icons.pause_rounded : Icons.play_arrow_rounded,
                          color: AppTheme.current.accent,
                          size: 24,
                        ),
                      ),
                    ),
                    SizedBox(width: 12),
                    Expanded(
                      child: Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: List.generate(_waveformHeights.length, (index) {
                          final heightRatio = _waveformHeights[index];
                          final isPlayed = index / _waveformHeights.length < _progress;
                          return Container(
                            width: 3,
                            height: 24 * heightRatio,
                            decoration: BoxDecoration(
                              color: isPlayed ? AppTheme.current.accent : AppTheme.current.mutedText.withValues(alpha: 0.5),
                              borderRadius: BorderRadius.circular(2),
                            ),
                          );
                        }),
                      ),
                    ),
                    SizedBox(width: 12),
                    Icon(Icons.mic, size: 16, color: AppTheme.current.accent.withValues(alpha: 0.6)),
                  ],
                ),
                SizedBox(height: 6),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text(
                      "${_formatDuration(_progress)} / ${_durationSeconds}s",
                      style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 9, fontFamily: 'monospace'),
                    ),
                    Text(
                      "${widget.timestamp.hour.toString().padLeft(2, '0')}:${widget.timestamp.minute.toString().padLeft(2, '0')}",
                      style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 8, fontFamily: 'monospace'),
                    ),
                  ],
                ),
              ],
            ),
          ),
          if (widget.reactions != null && widget.reactions!.isNotEmpty)
            Positioned(
              bottom: -8,
              left: widget.isMe ? null : 8,
              right: widget.isMe ? 8 : null,
              child: _buildReactionsRow(widget.reactions!),
            ),
        ],
      ),
    );
  }
}

// ==========================================
// 4. POLL BUBBLE WIDGET
// ==========================================
class PollBubble extends StatefulWidget {
  final String pollId;
  final String question;
  final List<String> options;
  final Map<String, List<String>> votes;
  final bool isMe;
  final DateTime timestamp;
  final String localPeerId;
  final Function(int) onVote;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const PollBubble({
    required this.pollId,
    required this.question,
    required this.options,
    required this.votes,
    required this.isMe,
    required this.timestamp,
    required this.localPeerId,
    required this.onVote,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    super.key,
  });


  @override
  State<PollBubble> createState() => _PollBubbleState();
}

class _PollBubbleState extends State<PollBubble> {
  @override
  Widget build(BuildContext context) {
    int totalVotes = widget.votes.values.fold(0, (sum, voters) => sum + voters.length);

    return Container(
      margin: EdgeInsets.only(top: 8, left: 12, right: 12, bottom: (widget.reactions != null && widget.reactions!.isNotEmpty) ? 22 : 8),
      alignment: widget.isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Container(
            constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
            padding: EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: const Color(0xFF1E2430),
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Icon(Icons.poll_rounded, color: AppTheme.current.accent, size: 18),
                    SizedBox(width: 8),
                    Text("POLL", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold)),
                  ],
                ),
                SizedBox(height: 8),
                Text(widget.question, style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
                SizedBox(height: 12),
                Column(
                  children: List.generate(widget.options.length, (index) {
                    final option = widget.options[index];
                    final voters = widget.votes[option] ?? [];
                    final hasVoted = voters.contains(widget.localPeerId);
                    final voteCount = voters.length;
                    final percentage = totalVotes > 0 ? voteCount / totalVotes : 0.0;
                    return GestureDetector(
                      onTap: () => widget.onVote(index),
                      child: Container(
                        margin: EdgeInsets.only(bottom: 8),
                        padding: EdgeInsets.all(10),
                        decoration: BoxDecoration(
                          color: hasVoted ? AppTheme.current.accent.withValues(alpha: 0.1) : AppTheme.current.text.withValues(alpha: 0.03),
                          borderRadius: BorderRadius.circular(8),
                          border: Border.all(color: hasVoted ? AppTheme.current.accent.withValues(alpha: 0.3) : AppTheme.current.mutedText.withValues(alpha: 0.1)),
                        ),
                        child: Stack(
                          children: [
                            FractionallySizedBox(
                              widthFactor: percentage,
                              child: Container(height: 24, decoration: BoxDecoration(color: AppTheme.current.accent.withValues(alpha: 0.08), borderRadius: BorderRadius.circular(4))),
                            ),
                            Row(
                              mainAxisAlignment: MainAxisAlignment.spaceBetween,
                              children: [
                                Text(option, style: TextStyle(color: hasVoted ? AppTheme.current.accent : AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13, fontFamily: 'monospace')),
                                Row(children: [if (hasVoted) Icon(Icons.check_circle, color: AppTheme.current.accent, size: 14), SizedBox(width: 4), Text("$voteCount", style: TextStyle(color: hasVoted ? AppTheme.current.accent : AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 12, fontWeight: FontWeight.bold))]),
                              ],
                            ),
                          ],
                        ),
                      ),
                    );
                  }),
                ),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text("$totalVotes votes", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontFamily: 'monospace')),
                    Text("${widget.timestamp.hour.toString().padLeft(2, '0')}:${widget.timestamp.minute.toString().padLeft(2, '0')}", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 8, fontFamily: 'monospace')),
                  ],
                ),
              ],
            ),
          ),
          if (widget.reactions != null && widget.reactions!.isNotEmpty)
            Positioned(
              bottom: -8,
              left: widget.isMe ? null : 8,
              right: widget.isMe ? 8 : null,
              child: _buildReactionsRow(widget.reactions!),
            ),
        ],
      ),
    );
  }
}

// ==========================================
// 5. STICKERS / EMOJIS / GIFS DRAWER PANEL
// ==========================================
class StickerEmojiPanel extends StatefulWidget {
  final Function(String emoji) onEmojiSelect;
  final Function(String stickerName) onStickerSelect;
  final Function(String gifUrl) onGifSelect;
  const StickerEmojiPanel({required this.onEmojiSelect, required this.onStickerSelect, required this.onGifSelect, super.key});
  @override
  State<StickerEmojiPanel> createState() => _StickerEmojiPanelState();
}

class _StickerEmojiPanelState extends State<StickerEmojiPanel> {
  final TextEditingController _gifSearchController = TextEditingController();
  List<String> _gifUrls = [];
  bool _loadingGifs = false;
  Timer? _debounceTimer;
  List<String> _customStickerPaths = [];

  @override
  void initState() { super.initState(); _fetchTrendingGifs(); _loadCustomStickers(); }

  Future<void> _loadCustomStickers() async {
    final prefs = await SharedPreferences.getInstance();
    final List<String> paths = prefs.getStringList('custom_stickers') ?? [];
    final List<String> existingPaths = paths.where((p) => File(p).existsSync()).toList();
    if (existingPaths.length != paths.length) await prefs.setStringList('custom_stickers', existingPaths);
    setState(() { _customStickerPaths = existingPaths; });
  }

  void _createBespokeSticker() async {
    try {
      final pickedFile = await ImagePicker().pickImage(source: ImageSource.gallery, imageQuality: 100);
      if (pickedFile == null) return;
      final appDir = await getApplicationDocumentsDirectory();
      final targetPath = "${appDir.path}/sticker_${DateTime.now().millisecondsSinceEpoch}.png";
      await File(pickedFile.path).copy(targetPath);
      final prefs = await SharedPreferences.getInstance();
      final List<String> paths = prefs.getStringList('custom_stickers') ?? [];
      paths.insert(0, targetPath);
      await prefs.setStringList('custom_stickers', paths);
      setState(() { _customStickerPaths = paths; });
    } catch (e) { debugPrint("Error: $e"); }
  }

  void _fetchTrendingGifs() async {
    final prefs = await SharedPreferences.getInstance();
    String apiKey = prefs.getString('klipy_api_key') ?? '';
    if (apiKey.isEmpty) { debugPrint('Klipy API key not configured'); if (mounted) setState(() => _loadingGifs = false); return; }
    setState(() => _loadingGifs = true);
    final client = HttpClient();
    try {
      final uri = Uri.parse('https://api.klipy.com/v2/featured?key=$apiKey&limit=24');
      final request = await client.getUrl(uri);
      final response = await request.close();
      if (response.statusCode == 200) {
        final body = await response.transform(utf8.decoder).join();
        final json = jsonDecode(body);
        final List<String> urls = [];
        for (var item in json['results'] ?? []) {
          String? url = item['media_formats']?['tinygif']?['url'] ?? item['src'];
          if (url != null) urls.add(url);
        }
        if (mounted) setState(() => _gifUrls = urls);
      }
    } finally { client.close(); if (mounted) setState(() => _loadingGifs = false); }
  }

  void _searchGifs(String query) async {
    if (query.isEmpty) { _fetchTrendingGifs(); return; }
    final prefs = await SharedPreferences.getInstance();
    String apiKey = prefs.getString('klipy_api_key') ?? '';
    if (apiKey.isEmpty) { debugPrint('Klipy API key not configured'); if (mounted) setState(() => _loadingGifs = false); return; }
    setState(() => _loadingGifs = true);
    final client = HttpClient();
    try {
      final uri = Uri.parse('https://api.klipy.com/v2/search?q=${Uri.encodeComponent(query)}&key=$apiKey&limit=24');
      final request = await client.getUrl(uri);
      final response = await request.close();
      if (response.statusCode == 200) {
        final body = await response.transform(utf8.decoder).join();
        final json = jsonDecode(body);
        final List<String> urls = [];
        for (var item in json['results'] ?? []) {
          String? url = item['media_formats']?['tinygif']?['url'] ?? item['src'];
          if (url != null) urls.add(url);
        }
        if (mounted) setState(() => _gifUrls = urls);
      }
    } finally { client.close(); if (mounted) setState(() => _loadingGifs = false); }
  }

  @override
  Widget build(BuildContext context) {
    return DefaultTabController(length: 3, child: Container(height: 320, color: AppTheme.current.surface, child: Column(children: [TabBar(indicatorColor: AppTheme.current.accent, labelColor: AppTheme.current.accent, unselectedLabelColor: AppTheme.current.mutedText, tabs: [Tab(icon: Icon(Icons.emoji_emotions_outlined), text: "EMOJIS"), Tab(icon: Icon(Icons.face_retouching_natural), text: "STICKERS"), Tab(icon: Icon(Icons.gif_box_outlined), text: "GIFS")]), Expanded(child: TabBarView(children: [_buildEmojiTab(), _buildStickersTab(), _buildGifsTab()]))])));
  }

  Widget _buildEmojiTab() {
    final List<String> emojis = ["😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😇", "😉", "😊", "😋", "😎", "😍", "😘", "👍", "👎", "👌", "✌️", "🤞", "🤟", "🤘", "🤙", "👈", "👉", "👇", "☝️", "🤝", "🔥", "💡", "🛡️", "🔑", "🔐", "🔒", "🌐", "💎", "💻", "🧠", "⚡", "🌟", "🎉", "❤️", "💔"];
    return GridView.builder(padding: EdgeInsets.all(12), gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(crossAxisCount: 8, crossAxisSpacing: 10, mainAxisSpacing: 10), itemCount: emojis.length, itemBuilder: (context, index) => GestureDetector(onTap: () => widget.onEmojiSelect(emojis[index]), child: Center(child: Text(emojis[index], style: TextStyle(fontSize: 26)))));
  }

  Widget _buildStickersTab() {
    final List<String> stickers = [r"$$$", "100", "BAKA", "BALLS", "CHICKENJOCKEY", "FLEXING", "GOAT", "MEWING", "NO", "SIGMA", "STEALING", "STOP", "SUS", "UNC", "eww", "lowkey", "mid", "nocap", "bruh", "cap", "delulu", "fahnio", "fahnumtax", "gyatt", "noob", "skibidi", "10-3", "6-7"];
    final total = 1 + _customStickerPaths.length + stickers.length;
    return GridView.builder(padding: EdgeInsets.all(12), gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(crossAxisCount: 4, crossAxisSpacing: 12, mainAxisSpacing: 12), itemCount: total, itemBuilder: (context, index) {
      if (index == 0) return GestureDetector(onTap: _createBespokeSticker, child: Container(decoration: BoxDecoration(color: AppTheme.current.accent.withValues(alpha: 0.05), borderRadius: BorderRadius.circular(12), border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2))), child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [Icon(Icons.add_photo_alternate_rounded, color: AppTheme.current.accent, size: 28), Text("BESPOKE", style: TextStyle(color: AppTheme.current.accent, fontSize: 8, fontWeight: FontWeight.bold))])));
      if (index <= _customStickerPaths.length) {
        final p = _customStickerPaths[index-1];
        return GestureDetector(onTap: () => widget.onStickerSelect(p), child: Container(padding: EdgeInsets.all(4), decoration: BoxDecoration(color: AppTheme.current.text.withValues(alpha: 0.02), borderRadius: BorderRadius.circular(12)), child: Image.file(File(p), fit: BoxFit.contain)));
      }
      final name = stickers[index - 1 - _customStickerPaths.length];
      return GestureDetector(onTap: () => widget.onStickerSelect(name), child: Container(padding: EdgeInsets.all(4), decoration: BoxDecoration(color: AppTheme.current.text.withValues(alpha: 0.02), borderRadius: BorderRadius.circular(12)), child: Image.asset('assets/images/stickers/$name.png', fit: BoxFit.contain)));
    });
  }

  Widget _buildGifsTab() {
    return Column(children: [
      Padding(padding: EdgeInsets.all(12), child: TextField(controller: _gifSearchController, onChanged: (v) { _debounceTimer?.cancel(); _debounceTimer = Timer(const Duration(milliseconds: 600), () => _searchGifs(v)); }, style: TextStyle(color: AppTheme.current.text, fontSize: 13), decoration: InputDecoration(hintText: "Search GIFs...", hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)), filled: true, fillColor: AppTheme.current.text.withValues(alpha: 0.05), border: OutlineInputBorder(borderRadius: BorderRadius.circular(20), borderSide: BorderSide.none)))),
      Expanded(child: _loadingGifs ? Center(child: CircularProgressIndicator(color: AppTheme.current.accent)) : GridView.builder(padding: EdgeInsets.all(12), gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(crossAxisCount: 2, crossAxisSpacing: 10, mainAxisSpacing: 10, childAspectRatio: 1.3), itemCount: _gifUrls.length, itemBuilder: (context, index) => GestureDetector(onTap: () => widget.onGifSelect(_gifUrls[index]), child: ClipRRect(borderRadius: BorderRadius.circular(10), child: Image.network(_gifUrls[index], fit: BoxFit.cover)))))
    ]);
  }
}

// ==========================================
// 6. LOCATION BUBBLE WIDGET
// ==========================================
class LocationBubble extends StatelessWidget {
  final double latitude;
  final double longitude;
  final bool isMe;
  final DateTime timestamp;
  final List<dynamic>? reactions;
  final String? msgId;
  final VoidCallback? onReactionTap;

  const LocationBubble({
    required this.latitude,
    required this.longitude,
    required this.isMe,
    required this.timestamp,
    this.reactions,
    this.msgId,
    this.onReactionTap,
    super.key,
  });

  Future<void> _openMap() async {
    final String url = Platform.isIOS ? "https://maps.apple.com/?q=$latitude,$longitude" : "https://www.google.com/maps/search/?api=1&query=$latitude,$longitude";
    if (await canLaunchUrl(Uri.parse(url))) await launchUrl(Uri.parse(url), mode: LaunchMode.externalApplication);
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: EdgeInsets.only(top: 6, left: 12, right: 12, bottom: (reactions != null && reactions!.isNotEmpty) ? 20 : 6),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Container(
            constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
            padding: EdgeInsets.all(12),
            decoration: BoxDecoration(color: const Color(0xFF161B26), borderRadius: BorderRadius.circular(16), border: Border.all(color: Colors.redAccent.withValues(alpha: 0.15))),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(children: [Icon(Icons.location_on_rounded, color: Colors.redAccent, size: 18), SizedBox(width: 8), Text("LOCATION SHARE", style: TextStyle(color: Colors.redAccent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold))]),
                SizedBox(height: 10),
                GestureDetector(onTap: _openMap, child: ClipRRect(borderRadius: BorderRadius.circular(8), child: Container(height: 140, width: double.infinity, color: Colors.black26, child: FlutterMap(options: MapOptions(initialCenter: LatLng(latitude, longitude), initialZoom: 14.0), children: [TileLayer(urlTemplate: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}.png", subdomains: ['a', 'b', 'c', 'd']), MarkerLayer(markers: [Marker(point: LatLng(latitude, longitude), child: Icon(Icons.location_on_rounded, color: Colors.redAccent, size: 30))])])))),
                SizedBox(height: 12),
                SizedBox(width: double.infinity, height: 36, child: ElevatedButton.icon(onPressed: _openMap, icon: Icon(Icons.open_in_new_rounded, size: 14, color: Colors.black), label: Text("VIEW ON MAP", style: TextStyle(color: Colors.black, fontSize: 11, fontWeight: FontWeight.bold)), style: ElevatedButton.styleFrom(backgroundColor: Colors.redAccent))),
                Align(alignment: Alignment.bottomRight, child: Text("${timestamp.hour}:${timestamp.minute.toString().padLeft(2, '0')}", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 9))),
              ],
            ),
          ),
          if (reactions != null && reactions!.isNotEmpty)
            Positioned(
              bottom: -8,
              left: isMe ? null : 8,
              right: isMe ? 8 : null,
              child: _buildReactionsRow(reactions!),
            ),
        ],
      ),
    );
  }
}
