import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:image_picker/image_picker.dart';
import 'package:geolocator/geolocator.dart';
import 'package:path_provider/path_provider.dart';
import 'package:image/image.dart' as img;
import '../src/native/introvert_client.dart';
import '../src/ui/widgets/rewards_hud.dart';
import '../src/ui/widgets/file_transfer_bubble.dart';
import '../src/ui/widgets/image_stack_bubble.dart';
import '../src/ui/widgets/note_bubble.dart';
import '../blueprint_ui.dart';
import 'call_screen.dart';
import 'chat_features.dart';
import 'media_gallery_viewer.dart';
import 'location_picker_screen.dart';
import 'package:record/record.dart';
import 'package:intl/intl.dart';
import '../theme/app_theme.dart';
import '../src/services/webrtc_call_service.dart';

class MessageModel {
  final String content;
  final bool isMe;
  final DateTime timestamp;
  int status; // 0=Sent, 1=Delivered, 2=Read
  final String? msgId;
  final String? replyTo;

  MessageModel({
    required this.content,
    required this.isMe,
    required this.timestamp,
    this.status = 0,
    this.msgId,
    this.replyTo,
  });
}

class _ContactInfoDialog extends StatefulWidget {
  final String peerId;
  final String peerName;
  final String? avatarBase64;
  final BuildContext parentContext;

  const _ContactInfoDialog({required this.peerId, required this.peerName, this.avatarBase64, required this.parentContext});

  @override
  State<_ContactInfoDialog> createState() => _ContactInfoDialogState();
}

class _ContactInfoDialogState extends State<_ContactInfoDialog> {
  final IntrovertClient _client = IntrovertClient();
  int _retentionSeconds = 0;
  String? _handle;
  String? _globalName;

  @override
  void initState() {
    super.initState();
    _loadContactInfo();
  }

  void _loadContactInfo() {
    final contacts = _client.getContacts();
    final contact = contacts.firstWhere((c) => c['peer_id'] == widget.peerId, orElse: () => null);
    if (contact != null) {
      setState(() {
        _retentionSeconds = contact['retention_hours'] as int? ?? 0;
        _handle = contact['handle'] as String?;
        _globalName = contact['global_name'] as String?;
      });
    }
  }

  void _showRetentionPicker() {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: EdgeInsets.all(20),
            child: Text("DISAPPEARING MESSAGES", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)),
          ),
          _buildRetentionOption("1 Minute", 60),
          _buildRetentionOption("5 Minutes", 300),
          _buildRetentionOption("30 Minutes", 1800),
          _buildRetentionOption("1 Hour", 3600),
          _buildRetentionOption("24 Hours", 86400),
          _buildRetentionOption("48 Hours", 172800),
          _buildRetentionOption("7 Days", 604800),
          _buildRetentionOption("14 Days", 1209600),
          _buildRetentionOption("Never", 0),
          SizedBox(height: 20),
        ],
      ),
    );
  }

  Widget _buildRetentionOption(String label, int seconds) {
    return Material(
      color: Colors.transparent,
      child: ListTile(
        title: Text(label, style: TextStyle(color: AppTheme.current.text)),
        trailing: _retentionSeconds == seconds ? Icon(Icons.check, color: AppTheme.current.accent) : null,
        onTap: () {
          _client.setRetention(widget.peerId, seconds, false);
          setState(() => _retentionSeconds = seconds);
          Navigator.pop(context);
        },
      ),
    );
  }

  String _formatRetention(int seconds) {
    if (seconds == 0) return "Off";
    if (seconds < 3600) return "${seconds ~/ 60} minutes";
    if (seconds < 86400) return "${seconds ~/ 3600} hours";
    return "${seconds ~/ 86400} days";
  }

  void _showInChatSearch(BuildContext dialogContext) {
    final searchController = TextEditingController();
    List<dynamic> searchResults = [];
    String query = '';
    Timer? debounceTimer;

    showDialog(
      context: dialogContext,
      barrierDismissible: false,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) {
          return AlertDialog(
            backgroundColor: AppTheme.current.surface,
            title: Row(
              children: [
                Icon(Icons.search, color: AppTheme.current.accent, size: 20),
                SizedBox(width: 8),
                Text("Search in Chat", style: TextStyle(color: AppTheme.current.text, fontSize: 16)),
              ],
            ),
            content: SizedBox(
              width: double.maxFinite,
              height: 400,
              child: Column(
                children: [
                  TextField(
                    controller: searchController,
                    autofocus: true,
                    style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                    onChanged: (val) {
                      query = val;
                      if (val.isEmpty) {
                        debounceTimer?.cancel();
                        setDialogState(() => searchResults = []);
                        return;
                      }
                      debounceTimer?.cancel();
                      debounceTimer = Timer(Duration(milliseconds: 300), () {
                        try {
                          final msgs = _client.getMessages(widget.peerId);
                          final q = val.toLowerCase();
                          searchResults = msgs.where((m) {
                            final content = (m['content'] as String? ?? '').toLowerCase();
                            return content.contains(q);
                          }).toList();
                          setDialogState(() {});
                        } catch (_) {}
                      });
                    },
                    decoration: InputDecoration(
                      hintText: "Search messages...",
                      hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                      prefixIcon: Icon(Icons.search, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                      filled: true,
                      fillColor: AppTheme.current.text.withValues(alpha: 0.04),
                      border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
                    ),
                  ),
                  SizedBox(height: 8),
                  if (query.isNotEmpty)
                    Row(
                      children: [
                        Icon(Icons.search, size: 14, color: AppTheme.current.accent),
                        SizedBox(width: 6),
                        Text(
                          '${searchResults.length} result${searchResults.length == 1 ? '' : 's'}',
                          style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.w600),
                        ),
                      ],
                    ),
                  SizedBox(height: 8),
                  Expanded(
                    child: searchResults.isEmpty
                        ? Center(
                            child: Text(
                              query.isEmpty ? 'Type to search messages' : 'No results found',
                              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                            ),
                          )
                        : ListView.builder(
                            itemCount: searchResults.length,
                            itemBuilder: (ctx, i) {
                              final msg = searchResults[i];
                              final content = msg['content'] as String? ?? '';
                              final isMe = msg['is_me'] == true || msg['is_me'] == 1 || msg['is_me'] == '1';
                              final timestamp = msg['timestamp'] as String? ?? '';

                              return Container(
                                margin: EdgeInsets.only(bottom: 4),
                                padding: EdgeInsets.all(8),
                                decoration: BoxDecoration(
                                  color: AppTheme.current.text.withValues(alpha: 0.04),
                                  borderRadius: BorderRadius.circular(8),
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      children: [
                                        Text(
                                          isMe ? 'You' : widget.peerName,
                                          style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold),
                                        ),
                                        Spacer(),
                                        Text(
                                          timestamp.length > 16 ? timestamp.substring(11, 16) : timestamp,
                                          style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 10),
                                        ),
                                      ],
                                    ),
                                    SizedBox(height: 4),
                                    Text(
                                      content,
                                      style: TextStyle(color: AppTheme.current.text, fontSize: 12),
                                      maxLines: 3,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                  ],
                                ),
                              );
                            },
                          ),
                  ),
                ],
              ),
            ),
            actions: [
              TextButton(onPressed: () { debounceTimer?.cancel(); searchController.dispose(); Navigator.pop(ctx); }, child: Text("CLOSE")),
            ],
          );
        },
      ),
    );
  }

  void _showMediaLinksDocs(BuildContext dialogContext) {
    // Categorize all messages into Media, Links, Docs
    final messages = _client.getMessages(widget.peerId);
    final List<dynamic> mediaItems = [];
    final List<dynamic> linkItems = [];
    final List<dynamic> docItems = [];

    for (var msg in messages) {
      final content = msg['content'] as String? ?? '';
      final timestamp = msg['timestamp'] as String? ?? '';

      if (content.startsWith('[FILE]:')) {
        try {
          final jsonStr = content.substring(7);
          final meta = json.decode(jsonStr);
          final filename = (meta['filename'] ?? '').toString().toLowerCase();
          final ext = filename.contains('.') ? filename.split('.').last : '';

          if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext) ||
              ['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext) ||
              ['mp3', 'wav', 'm4a', 'flac', 'ogg'].contains(ext)) {
            mediaItems.add({'type': 'media', 'filename': meta['filename'] ?? 'file', 'timestamp': timestamp, 'mime': meta['mime_type'] ?? ''});
          } else {
            docItems.add({'type': 'doc', 'filename': meta['filename'] ?? 'file', 'timestamp': timestamp, 'size': meta['total_size'] ?? 0});
          }
        } catch (_) {
          docItems.add({'type': 'doc', 'filename': 'file', 'timestamp': timestamp});
        }
      } else if (content.startsWith('[STICKER]:') || content.startsWith('[GIF]:')) {
        mediaItems.add({'type': 'media', 'filename': content.startsWith('[STICKER]:') ? 'Sticker' : 'GIF', 'timestamp': timestamp});
      } else if (content.startsWith('[NOTE]:')) {
        final title = content.substring(7).split('\n').first;
        docItems.add({'type': 'doc', 'filename': 'Note: $title', 'timestamp': timestamp});
      } else if (content.contains('http://') || content.contains('https://')) {
        final urlPattern = RegExp(r'https?://[^\s]+');
        final matches = urlPattern.allMatches(content);
        for (var match in matches) {
          linkItems.add({'type': 'link', 'url': content.substring(match.start, match.end), 'timestamp': timestamp});
        }
      }
    }

    final totalItems = mediaItems.length + linkItems.length + docItems.length;

    showModalBottomSheet(
      context: dialogContext,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => DefaultTabController(
        length: 3,
        child: SizedBox(
          height: MediaQuery.of(ctx).size.height * 0.7,
          child: Column(
            children: [
              SizedBox(height: 12),
              Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
              Padding(
                padding: EdgeInsets.all(16),
                child: Row(
                  children: [
                    Icon(Icons.perm_media_outlined, color: AppTheme.current.accent, size: 20),
                    SizedBox(width: 8),
                    Text('SHARED CONTENT', style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13, letterSpacing: 1.2)),
                    Spacer(),
                    Text('$totalItems items', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                  ],
                ),
              ),
              TabBar(
                labelColor: AppTheme.current.accent,
                unselectedLabelColor: AppTheme.current.mutedText,
                indicatorColor: AppTheme.current.accent,
                tabs: [
                  Tab(text: 'Media (${mediaItems.length})'),
                  Tab(text: 'Links (${linkItems.length})'),
                  Tab(text: 'Docs (${docItems.length})'),
                ],
              ),
              Expanded(
                child: TabBarView(
                  children: [
                    _buildSharedList(mediaItems, 'media'),
                    _buildSharedList(linkItems, 'links'),
                    _buildSharedList(docItems, 'docs'),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildSharedList(List<dynamic> items, String type) {
    if (items.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(type == 'media' ? Icons.photo_library_outlined : type == 'links' ? Icons.link : Icons.description_outlined,
              size: 48, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
            SizedBox(height: 12),
            Text('No $type shared yet', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
          ],
        ),
      );
    }
    return ListView.builder(
      padding: EdgeInsets.all(12),
      itemCount: items.length,
      itemBuilder: (ctx, i) {
        final item = items[i];
        return Container(
          margin: EdgeInsets.only(bottom: 6),
          decoration: BoxDecoration(
            color: AppTheme.current.text.withValues(alpha: 0.04),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Material(
            color: Colors.transparent,
            child: ListTile(
              dense: true,
              leading: Icon(
                type == 'media' ? Icons.image_outlined : type == 'links' ? Icons.link : Icons.description_outlined,
                color: type == 'media' ? Colors.blueAccent : type == 'links' ? Colors.cyanAccent : Colors.orangeAccent,
                size: 20,
              ),
              title: Text(
                item['filename'] ?? item['url'] ?? '',
                style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              subtitle: Text(
                (item['timestamp']?.toString().length ?? 0) > 16 ? item['timestamp'].toString().substring(0, 16) : (item['timestamp'] ?? ''),
                style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10),
              ),
            ),
          ),
        );
      },
    );
  }

  void _showElevatedMessages(BuildContext dialogContext) {
    final elevated = _client.getElevatedMessages(widget.peerId);

    showModalBottomSheet(
      context: dialogContext,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) {
          return SizedBox(
            height: MediaQuery.of(ctx).size.height * 0.6,
            child: Column(
              children: [
                SizedBox(height: 12),
                Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
                Padding(
                  padding: EdgeInsets.all(16),
                  child: Row(
                    children: [
                      Icon(Icons.push_pin, color: AppTheme.current.accent, size: 20),
                      SizedBox(width: 8),
                      Text('ELEVATED MESSAGES', style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13, letterSpacing: 1.2)),
                      Spacer(),
                      Text('${elevated.length} pinned', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                    ],
                  ),
                ),
                Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                Expanded(
                  child: elevated.isEmpty
                      ? Center(
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            children: [
                              Icon(Icons.push_pin_outlined, size: 48, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                              SizedBox(height: 12),
                              Text('No elevated messages', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                              SizedBox(height: 4),
                              Text('Long-press a message and tap "Elevate"', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3), fontSize: 11)),
                            ],
                          ),
                        )
                      : ListView.builder(
                          padding: EdgeInsets.all(12),
                          itemCount: elevated.length,
                          itemBuilder: (ctx, i) {
                            final item = elevated[i];
                            final content = item['content'] as String? ?? '';
                            final isMe = item['is_me'] == true;
                            final msgId = item['msg_id'] as String? ?? '';

                            String senderName = isMe ? 'You' : (widget.peerName ?? 'Peer');
                            String displayContent = content;
                            if (content.startsWith('[FILE]:')) {
                              try {
                                final meta = json.decode(content.substring(7));
                                displayContent = '📎 ${meta['filename'] ?? 'file'}';
                              } catch (_) { displayContent = '📎 File'; }
                            } else if (content.startsWith('[NOTE]:')) {
                              displayContent = '📝 ${content.substring(7).split('\n').first}';
                            }

                            return GestureDetector(
                              onLongPress: () async {
                                final confirm = await showDialog<bool>(
                                  context: ctx,
                                  builder: (dCtx) => AlertDialog(
                                    backgroundColor: AppTheme.current.surface,
                                    title: Text("Unelevate Message?", style: TextStyle(color: AppTheme.current.text)),
                                    content: Text("Remove this message from elevated messages?", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13)),
                                    actions: [
                                      TextButton(onPressed: () => Navigator.pop(dCtx, false), child: Text("CANCEL")),
                                      TextButton(onPressed: () => Navigator.pop(dCtx, true), child: Text("UNELEVATE", style: TextStyle(color: Colors.orangeAccent))),
                                    ],
                                  ),
                                );
                                if (confirm == true) {
                                  _client.unelevateMessage(widget.peerId, msgId);
                                  setDialogState(() {
                                    elevated.removeWhere((e) => e['msg_id'] == msgId);
                                  });
                                }
                              },
                              child: Container(
                                margin: EdgeInsets.only(bottom: 6),
                                padding: EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: AppTheme.current.text.withValues(alpha: 0.04),
                                  borderRadius: BorderRadius.circular(10),
                                  border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.08)),
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      children: [
                                        Icon(Icons.push_pin, size: 12, color: AppTheme.current.accent),
                                        SizedBox(width: 4),
                                        Text(senderName, style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold)),
                                        Spacer(),
                                        Text(
                                          (item['timestamp']?.toString().length ?? 0) > 16 ? item['timestamp'].toString().substring(11, 16) : '',
                                          style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 10),
                                        ),
                                      ],
                                    ),
                                    SizedBox(height: 4),
                                    Text(
                                      displayContent,
                                      style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                                      maxLines: 3,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                    SizedBox(height: 4),
                                    Text('Long-press to unelevate', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3), fontSize: 9)),
                                  ],
                                ),
                              ),
                            );
                          },
                        ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final displayName = _globalName?.isNotEmpty == true ? _globalName! : widget.peerName;
    return AlertDialog(
      backgroundColor: AppTheme.current.surface,
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
          SovereignAvatar(radius: 60, avatar: widget.avatarBase64 != null ? MemoryImage(base64Decode(widget.avatarBase64!)) : null),
          SizedBox(height: 16),
          // Name
          Text(
            displayName,
            style: TextStyle(color: AppTheme.current.text, fontSize: 20, fontWeight: FontWeight.bold),
            textAlign: TextAlign.center,
          ),
          SizedBox(height: 6),
          // Handle (if registered)
          if (_handle != null && _handle!.isNotEmpty)
            Text(
              _handle!,
              style: TextStyle(color: AppTheme.current.accent, fontSize: 14, fontWeight: FontWeight.w500),
              textAlign: TextAlign.center,
            ),
          SizedBox(height: 6),
          // Peer ID
          Text(
            widget.peerId,
            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.6), fontSize: 11, fontFamily: 'monospace'),
            textAlign: TextAlign.center,
          ),
          SizedBox(height: 20),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.timer_outlined, color: AppTheme.current.accent),
              title: Text("Disappearing Messages", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
              subtitle: Text(_formatRetention(_retentionSeconds), style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              trailing: Icon(Icons.chevron_right, size: 18),
              onTap: _showRetentionPicker,
            ),
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.search, color: AppTheme.current.accent),
              title: Text("Search in Chat", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
              subtitle: Text("Find messages in this conversation", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              onTap: () {
                Navigator.pop(context);
                _showInChatSearch(widget.parentContext);
              },
            ),
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.perm_media_outlined, color: AppTheme.current.accent),
              title: Text("Media, Links & Docs", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
              subtitle: Text("View shared files and links", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              trailing: Icon(Icons.chevron_right, size: 18),
              onTap: () {
                Navigator.pop(context);
                _showMediaLinksDocs(widget.parentContext);
              },
            ),
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.push_pin, color: AppTheme.current.accent),
              title: Text("Elevated Messages", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
              subtitle: Text("View bookmarked messages", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              trailing: Icon(Icons.chevron_right, size: 18),
              onTap: () {
                Navigator.pop(context);
                _showElevatedMessages(widget.parentContext);
              },
            ),
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.sync, color: AppTheme.current.accent),
              title: Text("Sync Chat", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
              subtitle: Text("Fetch all contacts, profiles & messages from mesh", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              onTap: () {
                _client.pollPeerProfile(widget.peerId);
                _client.syncChatMessages(widget.peerId, widget.peerId, false, isFull: true);
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text("Syncing full chat...", style: TextStyle(color: AppTheme.current.accent)),
                    backgroundColor: AppTheme.current.surface,
                  ),
                );
              },
            ),
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: const Icon(Icons.delete_sweep, color: Colors.redAccent),
              title: const Text("Clear Chat", style: TextStyle(color: Colors.redAccent, fontSize: 13)),
              onTap: () async {
                final confirm = await showDialog<bool>(
                  context: context,
                  builder: (ctx) => AlertDialog(
                    backgroundColor: AppTheme.current.surface,
                    title: const Text("Clear Chat?", style: TextStyle(color: Colors.redAccent)),
                    content: Text("This will permanently delete all messages with this contact from your device.", style: TextStyle(color: AppTheme.current.text)),
                    actions: [
                      TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text("CANCEL")),
                      TextButton(onPressed: () => Navigator.pop(ctx, true), child: const Text("CLEAR", style: TextStyle(color: Colors.redAccent))),
                    ],
                  ),
                );
                if (confirm == true) {
                  _client.deleteChat(widget.peerId);
                  if (mounted) Navigator.pop(context, true);
                }
              },
            ),
          ),
        ],
      ),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: Text("CLOSE", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7)))),
      ],
    );
  }
}

class ChatScreen extends StatefulWidget {
  final String peerId;
  final String? peerName;
  final String? avatarBase64;
  final String? initialReplyToContent;
  final String? initialReplyToSender;

  const ChatScreen({required this.peerId, this.peerName, this.avatarBase64, this.initialReplyToContent, this.initialReplyToSender, super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final IntrovertClient _client = IntrovertClient();
  final List<dynamic> _messages = [];
  int _messagesVersion = 0;
  int _cachedDisplayVersion = -1;
  List<dynamic>? _cachedDisplayMessages;
  bool _isLoading = false;
  bool _isSyncing = false;
  bool _isDisposing = false;
  String? _myAvatar;
  dynamic _replyingTo;
  String? _editingMsgId;
  String _status = "DISCONNECTED";
  bool _isE2eeActive = false;
  double _relayedBytes = 0;
  double _solRewards = 0;
  int _myTier = 0;
  int _peerTier = 0;
  bool _isRecording = false;
  bool _showPanel = false;
  bool _isInputEmpty = true;
  double _recordingDuration = 0.0;
  Timer? _recordingTimer;
  Timer? _loadMessagesDebounce;
  final AudioRecorder _audioRecorder = AudioRecorder();
  dynamic _selectedMsg;
  
  String? _peerName;

  StreamSubscription? _networkSubscription;
  StreamSubscription? _economySubscription;
  StreamSubscription<FileTransferProgress>? _transferSubscription;
  final Map<String, Map<String, List<String>>> _polls = {};
  final Map<String, List<dynamic>> _reactionsCache = {}; // msgId -> reactions
  final Set<String> _pullRequested = {};
  final Map<String, DateTime> _pullRequestedAt = {};
  static const Duration _pullRetryTimeout = Duration(seconds: 30);
  int _elevatedCount = 0;
  Timer? _pullRetryTimer;
  DateTime? _lastNetworkOptimizeTime;

  @override
  void initState() {
    super.initState();
    _peerName = widget.peerName;
    _messageController.addListener(() {
      final empty = _messageController.text.trim().isEmpty;
      if (empty != _isInputEmpty) {
        setState(() => _isInputEmpty = empty);
      }
    });
    
    if (widget.initialReplyToContent != null) {
      _replyingTo = MessageModel(content: widget.initialReplyToContent!, isMe: false, timestamp: DateTime.now());
    }
    
    getApplicationDocumentsDirectory().then((dir) {
    });

    _loadProfile();
    _loadPeerTier();
    _loadMessages();
    _markMessagesAsRead();
    _startNetworkDiscovery();
    _startEconomyMonitor();

    // Periodic retry for stalled file pulls (every 30 seconds)
    _pullRetryTimer = Timer.periodic(Duration(seconds: 30), (_) {
      if (mounted) _loadMessages();
    });

    // Load more messages when scrolling to top
    _scrollController.addListener(() {
      if (_scrollController.position.pixels < 200 && _hasMoreMessages && !_isLoading) {
        _loadMoreMessages();
      }
    });
    
    // Graceful background update of peer profile
    _client.pollPeerProfile(widget.peerId);
    
    // Auto-sync: contacts + last 100 messages (background, discreet)
    setState(() => _isSyncing = true);
    _client.syncChatMessages(widget.peerId, widget.peerId, false);
    Future.delayed(Duration(seconds: 3), () {
      if (mounted) setState(() => _isSyncing = false);
    });
  }

  @override
  void dispose() {
    _isDisposing = true;
    _networkSubscription?.cancel();
    _economySubscription?.cancel();
    _transferSubscription?.cancel();
    _recordingTimer?.cancel();
    _pullRetryTimer?.cancel();
    _loadMessagesDebounce?.cancel();
    _audioRecorder.dispose();
    _messageController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _loadProfile() {
    final profile = _client.getProfile();
    setState(() {
      _myAvatar = profile['avatar'];
    });
  }

  void _loadPeerTier() {
    final contacts = _client.getContacts();
    final contact = contacts.firstWhere((c) => c['peer_id'] == widget.peerId, orElse: () => null);
    if (contact != null) {
      setState(() {
        _peerTier = contact['prestige_tier'] as int? ?? 0;
      });
    }
  }

  void _markMessagesAsRead() {
    // Mark all incoming messages as read locally
    _client.updateMessageStatusForPeer(widget.peerId, 0);
    // Send read receipts to the remote peer for each unread incoming message
    try {
      final raw = _client.getMessages(widget.peerId);
      for (var m in raw) {
        final isMe = m['is_me'] == true || m['is_me'] == 1 || m['is_me'] == '1';
        final msgId = m['msg_id'] as String?;
        final status = m['status'] as int? ?? 1;
        // Only send read receipt for incoming messages that haven't been marked read (status != 0)
        if (!isMe && msgId != null && msgId.isNotEmpty && status != 0) {
          _client.sendAcknowledgement(widget.peerId, msgId, 2);
        }
      }
    } catch (e) {
      debugPrint("Error sending read receipts: $e");
    }
  }

  void _debouncedLoadMessages() {
    _loadMessagesDebounce?.cancel();
    _loadMessagesDebounce = Timer(const Duration(milliseconds: 300), () {
      if (mounted) _loadMessages();
    });
  }

  static const int _pageSize = 100;
  int _loadedOffset = 0;
  bool _hasMoreMessages = true;

  Future<void> _loadMessages() async {
    setState(() => _isLoading = true);
    _loadedOffset = 0;
    _hasMoreMessages = true;
    try {
      final raw = _client.getMessagesPaginated(widget.peerId, offset: 0, limit: _pageSize);
      _hasMoreMessages = raw.length >= _pageSize;
      _loadedOffset = raw.length;
      final List<dynamic> loaded = [];
      for (var m in raw) {
        final content = m['content'] as String? ?? '';
        final timestampStr = m['timestamp'] as String? ?? '';
        final isMe = m['is_me'] == true || m['is_me'] == 1 || m['is_me'] == '1';
        int status = m['status'] as int? ?? 1;
        final msgId = m['msg_id'] as String?;
        final replyTo = m['reply_to'] as String?;
        
        DateTime ts = _parseTimestamp(timestampStr);

        if (content.startsWith("[FILE]:")) {
          try {
            final jsonStr = content.substring(7);
            final meta = json.decode(jsonStr);
            final progress = FileTransferProgress.fromJson(meta);
            final fileHash = meta['file_hash']?.toString() ?? '';
            final filename = meta['filename']?.toString() ?? 'unknown';

            String? localPath = progress.localPath;
            bool exists = false;
            
            // VITAL FIX: If the message thinks the file is missing, check the Sovereign Drive (where it's auto-organized)
            if (localPath == null || !File(localPath).existsSync()) {
              final driveInfo = _client.driveGetByHash(fileHash);
              if (driveInfo.containsKey('local_path')) {
                 final organizedPath = _client.resolveSandboxPath(driveInfo['local_path']?.toString()) ?? "";
                 if (organizedPath.isNotEmpty && File(organizedPath).existsSync()) {
                   localPath = organizedPath;
                   exists = true;
                 }
              }
            } else {
              exists = true;
            }

            // Recover MIME type if generic
            String mimeType = progress.mimeType;
            if (mimeType == 'application/octet-stream') {
              final ext = filename.split('.').last.toLowerCase();
              if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext)) {
                mimeType = 'image/$ext';
              } else if (['mp4', 'mov', 'avi', 'mkv', 'webm'].contains(ext)) {
                mimeType = 'video/$ext';
              } else if (ext == 'pdf') {
                mimeType = 'application/pdf';
              }
            }

            double progressVal = exists ? 1.0 : 0.0;
            bool isCompleteVal = exists;
            bool isVerifiedVal = exists;
            bool isWaiting = !isMe && !exists;

            final existingIdx = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
            if (existingIdx != -1) {
              final existing = _messages[existingIdx] as FileTransferProgress;
              if (!exists) {
                progressVal = existing.progress;
                isCompleteVal = existing.isComplete;
                isVerifiedVal = existing.isVerified;
                localPath = existing.localPath ?? localPath;
                isWaiting = existing.isWaitingForDownload;
              }
            }

            final updatedProgress = FileTransferProgress(
              transferId: progress.transferId,
              peerId: progress.peerId,
              filename: filename,
              mimeType: mimeType,
              fileHash: fileHash,
              progress: progressVal,
              speedBps: 0.0,
              isComplete: isCompleteVal,
              isVerified: isVerifiedVal,
              isOutgoing: isMe,
              isCancelled: false,
              localPath: localPath,
              startTimeMs: progress.startTimeMs,
              isWaitingForDownload: isWaiting,
              thumbnail: progress.thumbnail,
            );

            if (!isMe && !exists && existingIdx == -1) {
              final shouldPull = !_pullRequested.contains(progress.transferId) ||
                (_pullRequestedAt[progress.transferId]?.isBefore(DateTime.now().subtract(_pullRetryTimeout)) ?? false);
              if (shouldPull) {
                try {
                  final totalSize = (meta['total_size'] as num?)?.toInt() ?? 0;
                  final isRelayed = _status != "Direct P2P";
                  _pullRequested.add(progress.transferId);
                  _pullRequestedAt[progress.transferId] = DateTime.now();
                  _client.startPull(progress.peerId, progress.transferId, progress.filename, progress.mimeType, fileHash, totalSize, isRelayed, null);
                } catch (_) {}
              }
            }

            loaded.add(updatedProgress);
            continue;
          } catch (e) {
            debugPrint("Failed to parse file transfer progress from db: $e");
          }
        }

        loaded.add(MessageModel(
          content: content,
          isMe: isMe,
          timestamp: ts,
          status: status,
          msgId: msgId,
          replyTo: replyTo,
        ));
      }
      // Pre-fetch reactions for all messages (avoids sync FFI in build path)
      _reactionsCache.clear();
      for (var m in loaded) {
        final msgId = m is MessageModel ? m.msgId : (m is FileTransferProgress ? m.transferId : null);
        if (msgId != null && msgId.isNotEmpty) {
          try {
            _reactionsCache[msgId] = _client.getMessageReactions(msgId);
          } catch (_) {}
        }
      }

      setState(() {
        _messages.clear();
        _messages.addAll(loaded);
        _messagesVersion++;
        _isLoading = false;
      });
      _refreshElevatedCount();
      _scrollToBottom();
    } catch (e) {
      debugPrint("Error loading messages: $e");
      if (mounted) setState(() => _isLoading = false);
    }
  }

  Future<void> _loadMoreMessages() async {
    if (!_hasMoreMessages || _isLoading) return;
    try {
      final raw = _client.getMessagesPaginated(widget.peerId, offset: _loadedOffset, limit: _pageSize);
      if (raw.isEmpty) {
        _hasMoreMessages = false;
        return;
      }
      _hasMoreMessages = raw.length >= _pageSize;
      _loadedOffset += raw.length;

      final List<dynamic> older = [];
      for (var m in raw) {
        final content = m['content'] as String? ?? '';
        final timestampStr = m['timestamp'] as String? ?? '';
        final isMe = m['is_me'] == true || m['is_me'] == 1 || m['is_me'] == '1';
        int status = m['status'] as int? ?? 1;
        final msgId = m['msg_id'] as String?;
        final replyTo = m['reply_to'] as String?;
        DateTime ts = _parseTimestamp(timestampStr);
        if (content.startsWith("[FILE]:")) {
          try {
            final jsonStr = content.substring(7);
            final meta = json.decode(jsonStr);
            final progress = FileTransferProgress.fromJson(meta);
            older.add(progress);
            continue;
          } catch (_) {}
        }
        older.add(MessageModel(content: content, isMe: isMe, timestamp: ts, status: status, msgId: msgId, replyTo: replyTo));
      }

      if (mounted) {
        setState(() {
          _messages.insertAll(0, older);
          _messagesVersion++;
        });
      }
    } catch (e) {
      debugPrint("Error loading more messages: $e");
    }
  }

  void _refreshElevatedCount() {
    try {
      final elevated = _client.getElevatedMessages(widget.peerId);
      if (mounted) setState(() => _elevatedCount = elevated.length);
    } catch (_) {}
  }

  void _showElevatedMessages(BuildContext dialogContext) {
    final elevated = _client.getElevatedMessages(widget.peerId);

    showModalBottomSheet(
      context: dialogContext,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) {
          return SizedBox(
            height: MediaQuery.of(ctx).size.height * 0.6,
            child: Column(
              children: [
                SizedBox(height: 12),
                Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
                Padding(
                  padding: EdgeInsets.all(16),
                  child: Row(
                    children: [
                      Icon(Icons.push_pin, color: AppTheme.current.accent, size: 20),
                      SizedBox(width: 8),
                      Text('ELEVATED MESSAGES', style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontSize: 13, letterSpacing: 1.2)),
                      Spacer(),
                      Text('${elevated.length} pinned', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                    ],
                  ),
                ),
                Divider(height: 1, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                Expanded(
                  child: elevated.isEmpty
                      ? Center(
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            children: [
                              Icon(Icons.push_pin_outlined, size: 48, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                              SizedBox(height: 12),
                              Text('No elevated messages', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                              SizedBox(height: 4),
                              Text('Long-press a message and tap "Elevate"', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3), fontSize: 11)),
                            ],
                          ),
                        )
                      : ListView.builder(
                          padding: EdgeInsets.all(12),
                          itemCount: elevated.length,
                          itemBuilder: (ctx, i) {
                            final item = elevated[i];
                            final content = item['content'] as String? ?? '';
                            final isMe = item['is_me'] == true;
                            final msgId = item['msg_id'] as String? ?? '';

                            String senderName = isMe ? 'You' : (widget.peerName ?? 'Peer');
                            String displayContent = content;
                            if (content.startsWith('[FILE]:')) {
                              try {
                                final meta = json.decode(content.substring(7));
                                displayContent = '📎 ${meta['filename'] ?? 'file'}';
                              } catch (_) { displayContent = '📎 File'; }
                            } else if (content.startsWith('[NOTE]:')) {
                              displayContent = '📝 ${content.substring(7).split('\n').first}';
                            }

                            return GestureDetector(
                              onLongPress: () async {
                                final confirm = await showDialog<bool>(
                                  context: ctx,
                                  builder: (dCtx) => AlertDialog(
                                    backgroundColor: AppTheme.current.surface,
                                    title: Text("Unelevate Message?", style: TextStyle(color: AppTheme.current.text)),
                                    content: Text("Remove this message from elevated messages?", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13)),
                                    actions: [
                                      TextButton(onPressed: () => Navigator.pop(dCtx, false), child: Text("CANCEL")),
                                      TextButton(onPressed: () => Navigator.pop(dCtx, true), child: Text("UNELEVATE", style: TextStyle(color: Colors.orangeAccent))),
                                    ],
                                  ),
                                );
                                if (confirm == true) {
                                  _client.unelevateMessage(widget.peerId, msgId);
                                  setDialogState(() {
                                    elevated.removeWhere((e) => e['msg_id'] == msgId);
                                  });
                                  _refreshElevatedCount();
                                }
                              },
                              child: Container(
                                margin: EdgeInsets.only(bottom: 6),
                                padding: EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: AppTheme.current.text.withValues(alpha: 0.04),
                                  borderRadius: BorderRadius.circular(10),
                                  border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.08)),
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      children: [
                                        Icon(Icons.push_pin, size: 12, color: AppTheme.current.accent),
                                        SizedBox(width: 4),
                                        Text(senderName, style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold)),
                                        Spacer(),
                                        Text(
                                          (item['timestamp']?.toString().length ?? 0) > 16 ? item['timestamp'].toString().substring(11, 16) : '',
                                          style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 10),
                                        ),
                                      ],
                                    ),
                                    SizedBox(height: 4),
                                    Text(
                                      displayContent,
                                      style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                                      maxLines: 3,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                    SizedBox(height: 4),
                                    Text('Long-press to unelevate', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3), fontSize: 9)),
                                  ],
                                ),
                              ),
                            );
                          },
                        ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  bool _isNearBottom() {
    if (!_scrollController.hasClients) return true;
    final maxScroll = _scrollController.position.maxScrollExtent;
    final currentScroll = _scrollController.position.pixels;
    return (maxScroll - currentScroll) < 200;
  }

  void _scrollToBottom({bool force = false}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        // Only auto-scroll if user is already near the bottom (or forced)
        if (force || _isNearBottom()) {
          _scrollController.animateTo(
            _scrollController.position.maxScrollExtent,
            duration: const Duration(milliseconds: 300),
            curve: Curves.easeOut,
          );
        }
      }
    });
  }

  DateTime _parseTimestamp(String? ts) {
    if (ts == null || ts.isEmpty) return DateTime.now();
    try {
      // Handle SQLite format (YYYY-MM-DD HH:MM:SS) which is UTC
      String iso = ts.replaceAll(' ', 'T');
      if (!iso.contains('T')) {
        // Might be a Unix timestamp as string
        final intVal = int.tryParse(ts);
        if (intVal != null && intVal > 946684800) { // After 2000-01-01
          return DateTime.fromMillisecondsSinceEpoch(intVal * 1000).toLocal();
        }
        return DateTime.tryParse(ts) ?? DateTime.now();
      }
      if (!iso.endsWith('Z')) iso += 'Z';
      final parsed = DateTime.tryParse(iso)?.toLocal();
      // Guard against epoch date (0 = 1970-01-01)
      if (parsed != null && parsed.year < 2000) return DateTime.now();
      return parsed ?? DateTime.now();
    } catch (_) {
      return DateTime.now();
    }
  }

  bool _isImageFile(dynamic msg) {
    if (msg is! FileTransferProgress) return false;
    final progress = msg;
    final String ext = progress.filename.split('.').last.toLowerCase();
    final bool isImage = progress.mimeType.startsWith('image/') ||
        ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'heif'].contains(ext);
    return isImage;
  }

  List<dynamic> get _displayMessages {
    if (_cachedDisplayVersion == _messagesVersion && _cachedDisplayMessages != null) {
      return _cachedDisplayMessages!;
    }
    final List<dynamic> result = [];
    int i = 0;
    while (i < _messages.length) {
      final msg = _messages[i];
      if (_isImageFile(msg)) {
        final List<FileTransferProgress> group = [msg as FileTransferProgress];
        int j = i + 1;
        while (j < _messages.length) {
          final nextMsg = _messages[j];
          if (_isImageFile(nextMsg)) {
            final nextProg = nextMsg as FileTransferProgress;
            final currentProg = group.last;
            
            final bool sameSender = nextProg.isOutgoing == currentProg.isOutgoing && 
                nextProg.peerId == currentProg.peerId;
                
            final bool withinTime = (nextProg.startTimeMs - currentProg.startTimeMs).abs() < 10000;
            
            if (sameSender && withinTime) {
              group.add(nextProg);
              j++;
            } else {
              break;
            }
          } else {
            break;
          }
        }
        if (group.length > 1) {
          result.add(ImageGroupProgress(images: group));
        } else {
          result.add(group.first);
        }
        i = j;
      } else {
        result.add(msg);
        i++;
      }
    }
    _cachedDisplayMessages = result;
    _cachedDisplayVersion = _messagesVersion;
    return result;
  }

  void _startEconomyMonitor() {
    _economySubscription = _client.economyStream.listen((stats) {
      if (mounted) {
        setState(() {
          _solRewards = (stats['intr_balance'] ?? 0) / 1000000000.0;
          _myTier = _computeTier(_solRewards);
        });
      }
    });
  }

  static int _computeTier(double balance) {
    if (balance >= 1000000) return 4;
    if (balance >= 500000) return 3;
    if (balance >= 250000) return 2;
    if (balance >= 100000) return 1;
    return 0;
  }

  void _scrollToMessage(String? msgId) {
    if (msgId == null) return;
    final index = _messages.indexWhere((m) {
      if (m is MessageModel) return m.msgId == msgId;
      if (m is FileTransferProgress) return m.transferId == msgId;
      return false;
    });

    if (index != -1) {
      _scrollController.animateTo(
        index * 80.0, // Rough estimate, will be improved by ensuring the item is in view
        duration: const Duration(milliseconds: 500),
        curve: Curves.easeInOut,
      );
      // Highlight logic could be added here if needed
    }
  }

  Widget _buildBubbleContent(dynamic msg) {
    String? msgId;
    if (msg is MessageModel) {
      msgId = msg.msgId;
      final content = msg.content;
      final reactions = msgId != null ? (_reactionsCache[msgId] ?? []) : [];

      if (content.startsWith("[STICKER]:")) {
        return StickerBubble(
          name: content.substring(10),
          isMe: msg.isMe,
          timestamp: msg.timestamp,
          reactions: reactions,
          msgId: msgId,
          onReactionTap: () => _showReactionDetails(msgId!, reactions!),
        );
      }
      if (content.startsWith("[GIF]:")) {
        return GifBubble(
          url: content.substring(6),
          isMe: msg.isMe,
          timestamp: msg.timestamp,
          reactions: reactions,
          msgId: msgId,
          onReactionTap: () => _showReactionDetails(msgId!, reactions!),
        );
      }
      if (content.startsWith("[POLL_CREATE]:")) {
        try {
          final data = json.decode(content.substring(14));
          final pollId = data["poll_id"]?.toString() ?? '';
          return PollBubble(
            pollId: pollId,
            question: data["question"]?.toString() ?? '',
            options: List<String>.from(data["options"] ?? []),
            votes: _polls[pollId] ?? {},
            isMe: msg.isMe,
            timestamp: msg.timestamp,
            localPeerId: _client.localPeerId ?? '',
            onVote: (idx) => _votePoll(pollId, idx),
            reactions: reactions,
            msgId: msgId,
            onReactionTap: () => _showReactionDetails(msgId!, reactions!),
          );
        } catch (_) {}
      }
      if (content.startsWith("[NOTE]:")) {
        try {
          final noteData = content.substring(7);
          final firstNewline = noteData.indexOf('\n');
          final title = firstNewline > 0 ? noteData.substring(0, firstNewline) : noteData;
          final rest = firstNewline > 0 ? noteData.substring(firstNewline + 1) : '';
          // Format: [NOTE]:title\nimagePath\ncontent (if image) or [NOTE]:title\ncontent (no image)
          String? imagePath;
          String noteContent = rest;
          if (rest.isNotEmpty && !rest.startsWith('http') && (rest.endsWith('.jpg') || rest.endsWith('.jpeg') || rest.endsWith('.png') || rest.endsWith('.gif') || rest.endsWith('.webp') || rest.endsWith('.heic'))) {
            imagePath = rest;
            noteContent = '';
          } else if (rest.contains('\n')) {
            final secondNewline = rest.indexOf('\n');
            final possiblePath = rest.substring(0, secondNewline);
            if (possiblePath.endsWith('.jpg') || possiblePath.endsWith('.jpeg') || possiblePath.endsWith('.png') || possiblePath.endsWith('.gif') || possiblePath.endsWith('.webp') || possiblePath.endsWith('.heic')) {
              imagePath = possiblePath;
              noteContent = rest.substring(secondNewline + 1);
            }
          }
          return NoteBubble(
            title: title,
            content: noteContent,
            imagePath: imagePath,
            isMe: msg.isMe,
            timestamp: msg.timestamp,
            reactions: reactions,
            msgId: msgId,
            onReactionTap: () => _showReactionDetails(msgId!, reactions!),
          );
        } catch (_) {}
      }
      if (content.startsWith("[LOCATION]:")) {
        final locData = content.substring(11);
        final commaIdx = locData.indexOf(',');
        if (commaIdx > 0) {
          final lat = double.tryParse(locData.substring(0, commaIdx));
          final lng = double.tryParse(locData.substring(commaIdx + 1));
          if (lat != null && lng != null) {
            return LocationBubble(
              latitude: lat,
              longitude: lng,
              isMe: msg.isMe,
              timestamp: msg.timestamp,
              reactions: reactions,
              msgId: msgId,
              onReactionTap: (msgId != null && reactions != null && reactions.isNotEmpty) ? () => _showReactionDetails(msgId!, reactions) : null,
            );
          }
        }
      }
      
      dynamic replyTarget;
      ImageProvider? replyAvatar;
      if (msg.replyTo != null) {
        try {
          replyTarget = _messages.firstWhere((m) => (m is MessageModel && m.msgId == msg.replyTo) || (m is FileTransferProgress && m.transferId == msg.replyTo), orElse: () => null);
          if (replyTarget != null) {
            bool replyIsMe = false;
            if (replyTarget is MessageModel) replyIsMe = replyTarget.isMe;
            if (replyTarget is FileTransferProgress) replyIsMe = replyTarget.isOutgoing;
            
            if (replyIsMe) {
              if (_myAvatar != null) replyAvatar = MemoryImage(base64Decode(_myAvatar!));
            } else {
              if (widget.avatarBase64 != null) replyAvatar = MemoryImage(base64Decode(widget.avatarBase64!));
            }
          }
        } catch (_) {}
      }

      return GlassmorphicBubble(
        content: msg.content,
        isMe: msg.isMe,
        timestamp: msg.timestamp,
        status: msg.status,
        replyTo: replyTarget,
        replyAvatar: replyAvatar,
        onReplyTap: () => _scrollToMessage(msg.replyTo),
        reactions: reactions,
        msgId: msgId,
        onReactionTap: () => _showReactionDetails(msgId!, reactions!),
      );
    } else if (msg is FileTransferProgress) {
      msgId = msg.transferId;
      final reactions = _reactionsCache[msgId] ?? [];
      
      if (msg.filename.startsWith("voice_memo_")) {
        return VoiceMemoBubble(
          filename: msg.filename,
          isMe: msg.isOutgoing,
          timestamp: msg.startDateTime,
          localPath: msg.localPath ?? '',
          reactions: reactions,
          msgId: msgId,
          onReactionTap: reactions.isNotEmpty ? () => _showReactionDetails(msgId ?? '', reactions) : null,
        );
      }
      if (msg.filename.startsWith("sticker_")) {
        return StickerBubble(
          name: msg.localPath ?? msg.filename,
          isMe: msg.isOutgoing,
          timestamp: msg.startDateTime,
          reactions: reactions,
          msgId: msgId,
        );
      }
      return FileTransferBubble(
        key: ValueKey('${msg.transferId}_${msg.isVerified}_${msg.isComplete}'),
        progress: msg,
        isMe: msg.isOutgoing,
        reactions: reactions,
        allMessages: _messages,
        onTap: () {
          if (!msg.isComplete && !msg.isVerified && !msg.isOutgoing) {
            final msgs = _client.getMessages(widget.peerId);
            final directMsg = msgs.firstWhere(
              (m) => (m['content'] as String).startsWith("[FILE]:") && (m['content'] as String).contains(msg.transferId),
              orElse: () => null,
            );
            if (directMsg != null) {
              try {
                final content = directMsg['content'] as String;
                final meta = json.decode(content.substring(7));
                final fileHash = meta['file_hash']?.toString() ?? '';
                final totalSize = (meta['total_size'] as num?)?.toInt() ?? 0;
                final isRelayed = _status != "Direct P2P";
                if (!_pullRequested.contains(msg.transferId)) {
                  _pullRequested.add(msg.transferId);
                  _client.startPull(msg.peerId, msg.transferId, msg.filename, msg.mimeType, fileHash, totalSize, isRelayed, null);
                }
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(content: Text("Requesting '${msg.filename}' from mesh...")),
                );
              } catch (e) {
                debugPrint("Error parsing message meta for download: $e");
              }
            }
          }
        },
      );
    } else if (msg is ImageGroupProgress) {
      msgId = msg.images.first.transferId;
      final reactions = _reactionsCache[msgId] ?? [];
      return ImageStackBubble(
        group: msg,
        isMe: msg.isOutgoing,
        reactions: reactions,
        onTap: () {
          final List<FileTransferProgress> mediaList = [];
          for (var m in _messages) {
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

          if (mediaList.isEmpty) {
            mediaList.addAll(msg.images);
          }

          int initialIndex = mediaList.indexWhere((m) => m.transferId == msg.images.first.transferId);
          if (initialIndex == -1) initialIndex = 0;

          Navigator.of(context).push(
            MaterialPageRoute(
              builder: (context) => MediaGalleryViewer(
                mediaList: mediaList,
                initialIndex: initialIndex,
              ),
            ),
          );
        },
      );
    }
    return const SizedBox.shrink();
  }

  void _showReactionDetails(String msgId, List<dynamic> reactions) {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          Padding(
            padding: const EdgeInsets.all(16),
            child: Text("REACTIONS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
          ),
          Flexible(
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: reactions.length,
              itemBuilder: (context, index) {
                final r = reactions[index];
                final peerId = r['sender_id']?.toString() ?? 'Unknown';
                final emoji = r['emoji']?.toString() ?? '';
                final isMe = peerId == _client.localPeerId;
                
                String name = peerId;
                if (isMe) {
                  name = "You";
                } else {
                  try {
                    final contact = _client.getContacts().firstWhere((c) => c['peer_id'] == peerId, orElse: () => null);
                    if (contact != null) {
                      name = contact['alias'] ?? contact['global_name'] ?? peerId;
                    }
                  } catch (_) {}
                }

                return Material(color: Colors.transparent, child: ListTile(
                  leading: SovereignAvatar(
                    radius: 24,
                    prestigeTier: isMe ? _myTier : _peerTier,
                    avatar: isMe ? (_myAvatar != null ? MemoryImage(base64Decode(_myAvatar!)) : null) : (widget.avatarBase64 != null ? MemoryImage(base64Decode(widget.avatarBase64!)) : null),
                    initials: name == "You" ? "ME" : (name.isNotEmpty ? name[0].toUpperCase() : "?"),
                  ),
                  title: Text(name, style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                  subtitle: name != peerId && name != "You" ? Text(peerId, style: TextStyle(color: AppTheme.current.mutedText, fontSize: 9, fontFamily: 'monospace')) : null,
                  trailing: Text(emoji, style: const TextStyle(fontSize: 20)),
                ));
              },
            ),
          ),
          const SizedBox(height: 20),
        ],
      ),
    );
  }

  String _getSelectedMsgContent() {
    if (_selectedMsg == null) return '';
    if (_selectedMsg is MessageModel) return _selectedMsg.content;
    if (_selectedMsg is FileTransferProgress) return _selectedMsg.filename;
    return '';
  }

  String? _getSelectedMsgId() {
    if (_selectedMsg == null) return null;
    if (_selectedMsg is MessageModel) return _selectedMsg.msgId;
    if (_selectedMsg is FileTransferProgress) return _selectedMsg.transferId;
    return null;
  }

  PreferredSizeWidget _buildSelectionToolbar() {
    return AppBar(
      backgroundColor: AppTheme.current.accent.withValues(alpha: 0.15),
      leading: IconButton(
        icon: Icon(Icons.close, color: AppTheme.current.accent),
        onPressed: () => setState(() => _selectedMsg = null),
      ),
      title: Text("1 selected", style: TextStyle(color: AppTheme.current.accent, fontSize: 16, fontWeight: FontWeight.w500)),
      actions: [
        SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                icon: Icon(Icons.copy_rounded, color: AppTheme.current.accent),
                tooltip: 'Copy',
                onPressed: () {
                  final content = _getSelectedMsgContent();
                  if (content.isNotEmpty) {
                    Clipboard.setData(ClipboardData(text: content));
                    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Copied to clipboard")));
                  }
                  setState(() => _selectedMsg = null);
                },
              ),
              IconButton(
          icon: Icon(Icons.reply_rounded, color: AppTheme.current.accent),
          tooltip: 'Reply',
          onPressed: () {
            if (_selectedMsg != null) {
              setState(() {
                _replyingTo = _selectedMsg;
                _selectedMsg = null;
              });
            }
          },
        ),
        IconButton(
          icon: Icon(Icons.forward, color: AppTheme.current.accent),
          tooltip: 'Forward',
          onPressed: () {
            final content = _getSelectedMsgContent();
            setState(() => _selectedMsg = null);
            _showForwardDialog(content);
          },
        ),
        IconButton(
          icon: Icon(Icons.push_pin_outlined, color: AppTheme.current.accent),
          tooltip: 'Elevate',
          onPressed: () {
            final msgId = _getSelectedMsgId();
            final content = _getSelectedMsgContent();
            if (msgId != null) {
              final isMe = _selectedMsg is MessageModel ? (_selectedMsg as MessageModel).isMe : (_selectedMsg is FileTransferProgress ? (_selectedMsg as FileTransferProgress).isOutgoing : false);
              final senderId = isMe ? (_client.localPeerId ?? '') : widget.peerId;
              _client.elevateMessage(widget.peerId, msgId, content, senderId, isMe);
              _refreshElevatedCount();
              ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Message elevated")));
            }
            setState(() => _selectedMsg = null);
          },
        ),
        IconButton(
          icon: Icon(Icons.delete_outline_rounded, color: Colors.redAccent),
          tooltip: 'Delete',
          onPressed: () async {
            final msgId = _getSelectedMsgId();
            if (msgId == null) return;
            final confirm = await showDialog<bool>(
              context: context,
              builder: (ctx) => AlertDialog(
                backgroundColor: AppTheme.current.surface,
                title: Text("Delete Message?", style: TextStyle(color: AppTheme.current.text)),
                content: Text("This will delete the message for everyone in the chat.", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13)),
                actions: [
                  TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text("CANCEL")),
                  TextButton(onPressed: () => Navigator.pop(ctx, true), child: Text("DELETE", style: TextStyle(color: Colors.redAccent))),
                ],
              ),
            );
            if (confirm == true) {
              _client.deleteMessage(widget.peerId, msgId!, false);
              setState(() => _selectedMsg = null);
              _loadMessages();
            }
          },
        ),
            SizedBox(width: 8),
            ],
          ),
        ),
      ],
    );
  }

  void _showMessageActions(dynamic msg) {
    String content = "";
    String? msgId;
    bool isMe = false;
    DateTime ts = DateTime.now();

    if (msg is MessageModel) {
      content = msg.content;
      msgId = msg.msgId;
      isMe = msg.isMe;
      ts = msg.timestamp;
    } else if (msg is FileTransferProgress) {
      content = "[FILE]:${msg.localPath ?? ''}";
      msgId = msg.transferId;
      isMe = msg.isOutgoing;
      ts = msg.startDateTime;
    }

    if (msgId == null) return;

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          const SizedBox(height: 16),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceEvenly,
            children: [
              ...["👍", "❤️", "😂", "😮", "😢", "🙏"].map((emoji) => 
                GestureDetector(
                  onTap: () {
                    _client.sendReaction(widget.peerId, msgId!, emoji, false);
                    Navigator.pop(context);
                    _loadMessages();
                  },
                  child: Text(emoji, style: const TextStyle(fontSize: 24)),

                )
              ).toList(),
              GestureDetector(
                onTap: () {
                  Navigator.pop(context);
                  _showFullEmojiPicker(msgId!);
                },
                child: Container(
                  padding: const EdgeInsets.all(4),
                  decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), shape: BoxShape.circle),
                  child: Icon(Icons.add, color: AppTheme.current.accent, size: 20),
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1), height: 1),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: Icon(Icons.reply, color: AppTheme.current.accent),
              title: Text("Reply", style: TextStyle(color: AppTheme.current.text)),
              onTap: () {
                Navigator.pop(context);
                setState(() => _replyingTo = msg);
              },
            ),
          ),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: Icon(Icons.push_pin_outlined, color: AppTheme.current.accent),
              title: Text("Elevate", style: TextStyle(color: AppTheme.current.text)),
              subtitle: Text("Pin to elevated messages tab", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
              onTap: () {
                Navigator.pop(context);
                final senderId = isMe ? (_client.localPeerId ?? '') : widget.peerId;
                _client.elevateMessage(widget.peerId, msgId!, content, senderId, isMe);
                _refreshElevatedCount();
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(content: Text("Message elevated"), backgroundColor: AppTheme.current.surface),
                );
              },
            ),
          ),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: Icon(Icons.copy, color: AppTheme.current.accent),
              title: Text("Copy", style: TextStyle(color: AppTheme.current.text)),
              onTap: () {
                Navigator.pop(context);
                Clipboard.setData(ClipboardData(text: content));
                ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Copied to clipboard")));
              },
            ),
          ),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: Icon(Icons.forward, color: AppTheme.current.accent),
              title: Text("Forward", style: TextStyle(color: AppTheme.current.text)),
              onTap: () {
                Navigator.pop(context);
                _showForwardDialog(content);
              },
            ),
          ),
          if (isMe && DateTime.now().difference(ts).inSeconds <= 60 && msg is! FileTransferProgress)
            Material(
              color: Colors.transparent,
              child: ListTile(
                leading: Icon(Icons.edit, color: AppTheme.current.accent),
                title: Text("Edit", style: TextStyle(color: AppTheme.current.text)),
                onTap: () {
                  Navigator.pop(context);
                  setState(() {
                    _editingMsgId = msgId;
                    _messageController.text = content;
                  });
                },
              ),
            ),
          if (isMe)
            Material(
              color: Colors.transparent,
              child: ListTile(
                leading: Icon(Icons.delete, color: Colors.redAccent),
                title: Text("Delete", style: TextStyle(color: Colors.redAccent)),
                onTap: () async {
                  Navigator.pop(context);
                  final confirm = await showDialog<bool>(
                    context: context,
                    builder: (ctx) => AlertDialog(
                      backgroundColor: AppTheme.current.surface,
                      title: Text("Delete Message?", style: TextStyle(color: Colors.redAccent)),
                      content: Text("This will delete the message for everyone in the chat.", style: TextStyle(color: AppTheme.current.text)),
                      actions: [
                        TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text("CANCEL")),
                        TextButton(onPressed: () => Navigator.pop(ctx, true), child: Text("DELETE", style: TextStyle(color: Colors.redAccent))),
                      ],
                    ),
                  );
                  if (confirm == true) {
                    _client.deleteMessage(widget.peerId, msgId!, false);
                    setState(() {
                      _messages.removeWhere((m) {
                        if (m is FileTransferProgress) return m.transferId == msgId;
                        return (m is MessageModel && m.msgId == msgId);
                      });
                      _messagesVersion++;
                    });
                  }
                },
              ),
            ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }

  void _showFullEmojiPicker(String msgId) {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => Column(
        children: [
          const SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          Padding(
            padding: EdgeInsets.all(16.0),
            child: Text("ALL REACTIONS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
          ),
          Expanded(
            child: GridView.builder(
              padding: const EdgeInsets.all(16),
              gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(crossAxisCount: 7, mainAxisSpacing: 12, crossAxisSpacing: 12),
              itemCount: 80,
              itemBuilder: (context, index) {
                final List<String> emojis = ["😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😇", "😉", "😊", "😋", "😎", "😍", "😘", "😗", "😙", "😚", "☺️", "🙂", "🤗", "🤩", "🤔", "🤨", "😐", "😑", "😶", "🙄", "😏", "😣", "😥", "😮", "🤐", "😯", "😪", "😫", "😴", "😌", "😛", "😜", "😝", "🤤", "😒", "😓", "😔", "👍", "👎", "👌", "✌️", "🤞", "🤟", "🤘", "🤙", "👈", "👉", "👆", "🖕", "👇", "☝️", "🤝", "🔥", "💡", "🛡️", "🔑", "🔐", "🔒", "🌐", "💎", "💻", "🧠", "⚡", "🌟", "🎉", "❤️", "💔", "✨", "✅", "❌", "⚠️", "🚀"];
                if (index >= emojis.length) return const SizedBox.shrink();
                final emoji = emojis[index];
                return GestureDetector(
                  onTap: () {
                    _client.sendReaction(widget.peerId, msgId, emoji, false);
                    Navigator.pop(context);
                    _loadMessages();
                  },
                  child: Center(child: Text(emoji, style: const TextStyle(fontSize: 24))),
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  String _extractLocalPathFromProgress(FileTransferProgress progress) {
    String? localPath = progress.localPath;
    if (localPath != null && localPath.isNotEmpty) {
      localPath = _client.resolveSandboxPath(localPath);
    }
    if (localPath == null || !File(localPath).existsSync()) {
      if (progress.fileHash.isNotEmpty) {
        final driveInfo = _client.driveGetByHash(progress.fileHash);
        if (driveInfo.containsKey('local_path')) {
          final organizedPath = _client.resolveSandboxPath(driveInfo['local_path']?.toString()) ?? "";
          if (organizedPath.isNotEmpty && File(organizedPath).existsSync()) {
            localPath = organizedPath;
          }
        }
      }
    }
    return localPath ?? "";
  }

  String _extractLocalPath(String content) {
    if (!content.startsWith("[FILE]:")) return "";
    var pathOrJson = content.substring(7).trim();
    if (pathOrJson.startsWith("{")) {
      try {
        final meta = json.decode(pathOrJson);
        String? localPath = meta['local_path']?.toString();
        final fileHash = meta['file_hash']?.toString() ?? '';
        
        if (localPath != null && localPath.isNotEmpty) {
          localPath = _client.resolveSandboxPath(localPath);
        }
        if (localPath == null || !File(localPath).existsSync()) {
          final driveInfo = _client.driveGetByHash(fileHash);
          if (driveInfo.containsKey('local_path')) {
            final organizedPath = _client.resolveSandboxPath(driveInfo['local_path']?.toString()) ?? "";
            if (organizedPath.isNotEmpty && File(organizedPath).existsSync()) {
              localPath = organizedPath;
            }
          }
        }
        return localPath ?? "";
      } catch (_) {}
    } else {
      return _client.resolveSandboxPath(pathOrJson) ?? pathOrJson;
    }
    return "";
  }

  void _showForwardDialog(String content) {
    final contacts = _client.getContacts();
    final groups = _client.getAllGroups();

    final bool isFileForward = content.startsWith("[FILE]:") || _selectedMsg is FileTransferProgress;
    String? fileForwardPath;
    if (_selectedMsg is FileTransferProgress) {
      final prog = _selectedMsg as FileTransferProgress;
      final localPath = _extractLocalPathFromProgress(prog);
      if (localPath.isNotEmpty && File(localPath).existsSync()) {
        fileForwardPath = localPath;
      }
    } else if (content.startsWith("[FILE]:")) {
      final path = _extractLocalPath(content);
      if (path.isNotEmpty && File(path).existsSync()) {
        fileForwardPath = path;
      }
    }

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => DraggableScrollableSheet(
        initialChildSize: 0.6,
        minChildSize: 0.4,
        maxChildSize: 0.9,
        expand: false,
        builder: (_, scrollController) => Column(
          children: [
            Padding(
              padding: EdgeInsets.all(20),
              child: Text("FORWARD TO...", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
            ),
            Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
            Expanded(
              child: ListView(
                controller: scrollController,
                children: [
                  if (contacts.isNotEmpty) ...[
                    Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 8), child: Text("CONTACTS", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.bold))),
                    ...contacts.map((c) => Material(color: Colors.transparent, child: ListTile(
                      leading: SovereignAvatar(radius: 27, prestigeTier: c['prestige_tier'] as int? ?? 0, avatar: c['avatar'] != null ? MemoryImage(base64Decode(c['avatar'])) : null),
                      title: Text(c['alias'] ?? c['peer_id'], style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                      onTap: () {
                        if (isFileForward && fileForwardPath != null) {
                              _client.sendFile(c['peer_id'], fileForwardPath); 
                              Navigator.pop(ctx);
                              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("File forwarded")));
                        } else if (isFileForward) {
                              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Error: File not found locally")));
                        } else {
                           _client.sendMessage(c['peer_id'], content);
                           Navigator.pop(ctx);
                           ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Message forwarded")));
                        }
                      },
                    ))),
                  ],
                  if (groups.isNotEmpty) ...[
                    Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 8), child: Text("GROUPS", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.bold))),
                    ...groups.map((g) => Material(color: Colors.transparent, child: ListTile(
                      leading: SovereignAvatar(radius: 27, initials: g[1].isNotEmpty ? g[1][0].toUpperCase() : "G"),
                      title: Text(g[1], style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                      onTap: () {
                        if (isFileForward && fileForwardPath != null) {
                              final transferId = "gft_${DateTime.now().millisecondsSinceEpoch}";
                              _client.registerSeeder(transferId, fileForwardPath, "", File(fileForwardPath).lengthSync(), g[0]);
                              Navigator.pop(ctx);
                              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("File forwarded to group")));
                        } else if (isFileForward) {
                              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Error: File not found locally")));
                        } else {
                           _client.sendGroupMessage(g[0], content);
                           Navigator.pop(ctx);
                           ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Message forwarded to group")));
                        }
                      },
                    ))),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _startNetworkDiscovery() {
    _client.startNetwork();
    _client.establishSecureSession(widget.peerId);

    // Auto-recon: Let Intro-Claw optimize connection on chat start
    _runIntroClawRecon();

    _networkSubscription = _client.networkStream.listen((event) {
      if (_isDisposing) return;
      if (event.type == 8) {
        if (event.data.isEmpty) return;
        int colonIndex = event.data.lastIndexOf(58);
        if (colonIndex == -1 || colonIndex == event.data.length - 1) return;
        final eventPeerId = String.fromCharCodes(event.data.sublist(0, colonIndex));
        final statusCode = event.data[colonIndex + 1];
        if (eventPeerId != widget.peerId && eventPeerId.isNotEmpty) return;

        if (!mounted) return;
        setState(() {
          if (statusCode == 0) { _status = "Direct P2P"; }
          else if (statusCode == 1) { _status = "Relay Active"; }
          else { _status = "Offline"; _isE2eeActive = false; }
        });
        if (statusCode == 0 || statusCode == 1) _client.establishSecureSession(widget.peerId);

        // Show Intro-Claw connection confirmation
        if (statusCode == 0 && mounted) {
          _showIntroClawSnack("Intro-Claw: Direct P2P connection optimized", Colors.greenAccent);
        } else if (statusCode == 1 && mounted) {
          _showIntroClawSnack("Intro-Claw: Connected via relay — seeking direct path", Colors.orangeAccent);
        }
      } else if (event.type == 10) {
        // Network status event — check for weak/slow connection (RelayActive)
        if (event.data.isNotEmpty) {
          final statusCode = event.data[0];
          if (statusCode == 2 && mounted) {
            final now = DateTime.now();
            if (_lastNetworkOptimizeTime == null || now.difference(_lastNetworkOptimizeTime!) > const Duration(minutes: 2)) {
              _lastNetworkOptimizeTime = now;
              _client.forceNetworkRefresh();
              _showIntroClawSnack("Weak network detected... optimizing...", Colors.orangeAccent);
            }
          }
        }
      } else if (event.type == 2 || event.type == 4) {
        final data = event.data;
        if (data.length < 8) return;
        final timestamp = ByteData.sublistView(Uint8List.fromList(data.sublist(0, 8))).getInt64(0, Endian.big);
        String content; String? msgId; String? replyTo;
        if (event.type == 2) {
          if (data.length > 9) {
            int offset = 8;
            final midLen = data[offset++];
            msgId = utf8.decode(data.sublist(offset, offset + midLen));
            offset += midLen;
            if (data.length > offset) {
              final rtLen = data[offset++];
              replyTo = rtLen > 0 ? utf8.decode(data.sublist(offset, offset + rtLen)) : null;
              offset += rtLen;
            }
            content = utf8.decode(data.sublist(offset));
          } else { content = utf8.decode(data.sublist(8)); }
        } else { content = utf8.decode(data.sublist(8)); }

        if (content.startsWith("WEBRTC:")) return;
        if (!mounted) return;
        final eventTime = DateTime.fromMillisecondsSinceEpoch(timestamp * 1000);
        setState(() {
          if (event.type == 2) _isE2eeActive = true;
          if (content.startsWith("[FILE]:")) { _loadMessages(); return; }
          if (msgId != null) _client.sendAcknowledgement(widget.peerId, msgId, 2);
          
          final bool isDuplicate = msgId != null && msgId.isNotEmpty && _messages.any((m) => m is MessageModel && m.msgId == msgId);
          if (!isDuplicate) {
            final newMsg = MessageModel(content: content, isMe: false, timestamp: eventTime, status: msgId != null ? 2 : 1, msgId: msgId, replyTo: replyTo);
            int insertIdx = _messages.length;
            for (int i = _messages.length - 1; i >= 0; i--) {
              final m = _messages[i];
              DateTime? existingTs;
              if (m is MessageModel) existingTs = m.timestamp;
              else if (m is FileTransferProgress) existingTs = m.startDateTime;
              if (existingTs != null && existingTs.isAfter(eventTime)) {
                insertIdx = i;
              } else {
                break;
              }
            }
            _messages.insert(insertIdx, newMsg);
            _messagesVersion++;
          }
        });
        _scrollToBottom();
      } else if (event.type == 40) {
        if (mounted) setState(() {});
      } else if (event.type == 25) {
        if (!mounted || event.data.isEmpty) return;
        try {
          int offset = 0;
          final pidLen = event.data[offset++];
          if (offset + pidLen > event.data.length) return;
          final pId = utf8.decode(event.data.sublist(offset, offset + pidLen));
          offset += pidLen;

          if (pId == widget.peerId) {
              if (offset >= event.data.length) return;
              final nameLen = event.data[offset++];
              if (offset + nameLen > event.data.length) return;
              final name = utf8.decode(event.data.sublist(offset, offset + nameLen));
              offset += nameLen;
              if (offset >= event.data.length) return;
              final handleLen = event.data[offset++];
              if (offset + handleLen > event.data.length) return;
              offset += handleLen;
              if (offset + 4 > event.data.length) return;
              final avatarLen = (event.data[offset] << 24) | (event.data[offset + 1] << 16) | (event.data[offset + 2] << 8) | event.data[offset + 3];
              offset += 4 + avatarLen;
              final tier = offset < event.data.length ? event.data[offset] : 0;
              setState(() {
                _peerName = name;
                _peerTier = tier;
              });
          }
        } catch (_) {}
      } else if (event.type == 12) {
        if (!mounted) return;
        try {
          final progress = FileTransferProgress.fromJson(json.decode(utf8.decode(event.data)));
          // LEAKAGE FIX: Reject any group file transfer events — they belong in GroupChatScreen.
          // This mirrors the guard already present in _transferSubscription.
          if (progress.groupId != null && progress.groupId!.isNotEmpty) return;
          if (progress.peerId != widget.peerId && progress.peerId != _client.localPeerId) return;

          setState(() {
            final idx = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
            if (idx != -1) {
              _messages[idx] = progress;
            } else {
              // Only add if it's not a known message or manifest
              if (!_messages.any((m) => (m is MessageModel && m.content.contains(progress.transferId)))) {
                final fileTs = progress.startDateTime;
                int insertIdx = _messages.length;
                for (int i = _messages.length - 1; i >= 0; i--) {
                  final m = _messages[i];
                  DateTime? existingTs;
                  if (m is MessageModel) existingTs = m.timestamp;
                  else if (m is FileTransferProgress) existingTs = m.startDateTime;
                  if (existingTs != null && existingTs.isAfter(fileTs)) {
                    insertIdx = i;
                  } else {
                    break;
                  }
                }
                _messages.insert(insertIdx, progress);
                _messagesVersion++;
                _scrollToBottom();
              }
            }
          });
        } catch (_) {}
      } else if (event.type == 13) {

        if (!mounted) return;
        if (event.data.length < 2) return;
        final status = event.data[0];
        final mid = utf8.decode(event.data.sublist(1));
        final idx = _messages.indexWhere((m) => m is MessageModel && m.msgId == mid);
        if (idx != -1) {
          (_messages[idx] as MessageModel).status = status;
          if (mounted) setState(() {});
        }
      } else if (event.type == 37 || event.type == 38) {
        _debouncedLoadMessages();
      }
    });

    _transferSubscription = _client.transferStream.listen((progress) {
      if (!mounted) return;
      if (progress.groupId != null && progress.groupId!.isNotEmpty) return; // Leakage Fix: Skip group transfers
      if (progress.peerId != widget.peerId) return;

      // Ensure isOutgoing is correctly set for real-time updates
      // Rust usually sends peer_id as the target for outgoing, so it should match widget.peerId.
      // But we can be extra sure by checking if we have a matching outgoing message.
      setState(() {
        final idx = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
        if (progress.isComplete || progress.isCancelled) {
          _pullRequested.remove(progress.transferId);
        }
        if (idx != -1) {
          final existing = _messages[idx] as FileTransferProgress;
          // Preserve isOutgoing and localPath if we already had them
          final updated = FileTransferProgress(
            transferId: progress.transferId,
            peerId: progress.peerId,
            filename: progress.filename,
            mimeType: progress.mimeType,
            fileHash: progress.fileHash,
            progress: progress.progress,
            speedBps: progress.speedBps,
            isComplete: progress.isComplete,
            isVerified: progress.isVerified,
            isOutgoing: existing.isOutgoing || progress.isOutgoing,
            isCancelled: progress.isCancelled,
            localPath: existing.localPath ?? progress.localPath,
            startTimeMs: progress.startTimeMs,
            isWaitingForDownload: progress.isWaitingForDownload,
            thumbnail: existing.thumbnail ?? progress.thumbnail,
          );
          _messages[idx] = updated;
          _messagesVersion++;
        } else {
          _messages.add(progress);
          _messagesVersion++;
        }
      });
      _scrollToBottom();
    });
  }

  void _runIntroClawRecon() async {
    try {
      final report = _client.runNetworkRecon();
      if (!mounted) return;
      // Parse report for connection quality
      if (report.contains('OFFLINE') && widget.peerId.isNotEmpty) {
        _showIntroClawSnack("Intro-Claw: Peer offline — messages will be queued", Colors.orangeAccent);
      }
    } catch (_) {}
  }

  void _showIntroClawSnack(String message, Color color) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(message, style: TextStyle(color: color, fontSize: 12, fontWeight: FontWeight.w500)),
        backgroundColor: Color(0xFF001F2B),
        duration: Duration(seconds: 3),
        behavior: SnackBarBehavior.floating,
        margin: EdgeInsets.fromLTRB(16, 0, 16, 80),
      ),
    );
  }



  void _sendMessage() async {
    final text = _messageController.text.trim();
    if (text.isEmpty) return;

    if (_editingMsgId != null) {
      try {
        _client.editMessage(widget.peerId, _editingMsgId!, text, false);
        setState(() {
          _messageController.clear();
          _editingMsgId = null;
        });
        _loadMessages();
      } catch (e) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Edit failed: $e")));
      }
      return;
    }

    final replyToId = _replyingTo is MessageModel ? _replyingTo.msgId : (_replyingTo is FileTransferProgress ? _replyingTo.transferId : null);
    
    final String payload;
    if (replyToId == null && _replyingTo is MessageModel && _replyingTo.content.isNotEmpty) {
      final quoted = _replyingTo.content.length > 200 ? '${_replyingTo.content.substring(0, 200)}...' : _replyingTo.content;
      payload = '> $quoted\n\n$text';
    } else {
      payload = text;
    }
    
    try {
      final msgId = await _client.sendMessage(widget.peerId, payload, replyToId);
      if (!mounted) return;
      setState(() {
        _messages.add(MessageModel(content: payload, isMe: true, timestamp: DateTime.now(), status: 0, msgId: msgId, replyTo: replyToId));
        _messagesVersion++;
        _messageController.clear();
        _replyingTo = null;
        _relayedBytes += payload.length;
        _solRewards = _relayedBytes * 0.0000001;
      });
      _scrollToBottom();
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Send failed: $e")));
    }
  }

  void _sendSticker(String name) async {
    try {
      final payload = "[STICKER]:$name";
      final msgId = await _client.sendMessage(widget.peerId, payload);
      setState(() {
        _messages.add(MessageModel(content: payload, isMe: true, timestamp: DateTime.now(), status: 0, msgId: msgId));
        _messagesVersion++;
      });
      _scrollToBottom();
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to send sticker: $e")));
    }
  }

  void _sendGif(String url) async {
    try {
      final payload = "[GIF]:$url";
      final msgId = await _client.sendMessage(widget.peerId, payload);
      setState(() {
        _messages.add(MessageModel(content: payload, isMe: true, timestamp: DateTime.now(), status: 0, msgId: msgId));
      });
      _scrollToBottom();
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to send GIF: $e")));
    }
  }

  void _pickAndSendImage() async {
    try {
      final pickedFiles = await ImagePicker().pickMultiImage(imageQuality: 100);
      if (pickedFiles.isNotEmpty) {
        if (pickedFiles.length == 1) {
          final caption = await _showCaptionDialog();
          String path = pickedFiles.first.path;
          final ext = path.split('.').last.toLowerCase();
          if (ext == 'heic' || ext == 'heif') {
            path = await _convertHeicToJpeg(path);
          }
          _client.sendFile(widget.peerId, path);
          if (caption != null && caption.isNotEmpty) {
            _client.sendMessage(widget.peerId, caption);
          }
        } else {
          for (var file in pickedFiles) {
            String path = file.path;
            final ext = path.split('.').last.toLowerCase();
            if (ext == 'heic' || ext == 'heif') {
              path = await _convertHeicToJpeg(path);
            }
            _client.sendFile(widget.peerId, path);
          }
        }
      }
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to send image: $e")));
    }
  }

  void _pickAndSendVideo() async {
    try {
      final pickedFile = await ImagePicker().pickVideo(source: ImageSource.gallery);
      if (pickedFile != null) {
        final caption = await _showCaptionDialog();
        _client.sendFile(widget.peerId, pickedFile.path);
        if (caption != null && caption.isNotEmpty) {
          _client.sendMessage(widget.peerId, caption);
        }
      }
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to send video: $e")));
    }
  }

  void _sendFile() async {
    final result = await FilePicker.platform.pickFiles(type: FileType.any);
    if (result != null && result.files.single.path != null) {
      final caption = await _showCaptionDialog();
      _client.sendFile(widget.peerId, result.files.single.path!);
      if (caption != null && caption.isNotEmpty) {
        _client.sendMessage(widget.peerId, caption);
      }
    }
  }

  Future<String?> _showCaptionDialog() async {
    final captionController = TextEditingController();
    return showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        title: Text('Add a caption', style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold)),
        content: TextField(
          controller: captionController,
          style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'),
          decoration: InputDecoration(
            hintText: 'Write something...',
            hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
            filled: true,
            fillColor: AppTheme.current.text.withValues(alpha: 0.05),
            border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
            contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          ),
          autofocus: true,
          maxLines: 3,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: Text('Skip', style: TextStyle(color: AppTheme.current.mutedText)),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx, captionController.text),
            child: Text('Send', style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)),
          ),
        ],
      ),
    );
  }

  Future<String> _convertHeicToJpeg(String heicPath) async {
    try {
      final bytes = await File(heicPath).readAsBytes();
      final image = img.decodeImage(bytes);
      if (image != null) {
        final dir = await getApplicationDocumentsDirectory();
        final jpgPath = '${dir.path}/converted_${DateTime.now().millisecondsSinceEpoch}.jpg';
        await File(jpgPath).writeAsBytes(img.encodeJpg(image, quality: 90));
        return jpgPath;
      }
    } catch (e) {
      debugPrint("HEIC conversion failed: $e");
    }
    return heicPath; // Fallback to original
  }

  void _showInfo() async {
    final parentCtx = context;
    final result = await showDialog<bool>(
      context: parentCtx,
      builder: (context) => _ContactInfoDialog(
        peerId: widget.peerId,
        peerName: _peerName ?? "Peer",
        avatarBase64: widget.avatarBase64,
        parentContext: parentCtx,
      ),
    );
    if (result == true && mounted) {
      _loadMessages();
    }
  }

  void _startCall() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("START CALL", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              "Call ${_peerName ?? 'peer'}",
              style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                _buildCallTypeOption(
                  icon: Icons.phone_rounded,
                  label: "Audio",
                  color: AppTheme.current.accent,
                  onTap: () {
                    Navigator.pop(context);
                    _initiateCall(0);
                  },
                ),
                _buildCallTypeOption(
                  icon: Icons.videocam_rounded,
                  label: "Video",
                  color: AppTheme.current.accent,
                  onTap: () {
                    Navigator.pop(context);
                    _initiateCall(2);
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildCallTypeOption({
    required IconData icon,
    required String label,
    required Color color,
    required VoidCallback onTap,
  }) {
    return GestureDetector(
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 64,
            height: 64,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.15),
              shape: BoxShape.circle,
              border: Border.all(color: color.withValues(alpha: 0.5), width: 1.5),
            ),
            child: Icon(icon, color: color, size: 28),
          ),
          const SizedBox(height: 8),
          Text(
            label,
            style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12),
          ),
        ],
      ),
    );
  }

  void _initiateCall(int mediaType) async {
    final callService = WebRtcCallService.instance;

    // Check network before starting call
    final check = await callService.checkNetworkBeforeCall(mediaType != 0);
    if (!check.allowed) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(check.reason),
            backgroundColor: Colors.orangeAccent,
            duration: const Duration(seconds: 4),
          ),
        );
      }
      return;
    }

    // Use suggested media type if network is poor
    mediaType = check.suggestedMediaType;

    if (mounted && check.reason.isNotEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(check.reason),
          backgroundColor: Colors.orangeAccent.withValues(alpha: 0.9),
          duration: const Duration(seconds: 3),
        ),
      );
    }

    await callService.initialize();
    if (mounted) {
      Navigator.push(
        context,
        MaterialPageRoute(
          builder: (context) => CallScreen(
            peerId: widget.peerId,
            contactName: _peerName ?? "Peer",
            contactAvatar: widget.avatarBase64,
            isCaller: true,
          ),
        ),
      );
    }
  }

  void _showAttachmentOptions() {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      builder: (context) {
        return Container(
          decoration: BoxDecoration(
            color: AppTheme.current.surface.withValues(alpha: 0.95),
            borderRadius: const BorderRadius.vertical(top: Radius.circular(24)),
            border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1), width: 0.5),
          ),
          padding: const EdgeInsets.symmetric(vertical: 24, horizontal: 24),
          child: SafeArea(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 40,
                  height: 4,
                  margin: const EdgeInsets.only(bottom: 20),
                  decoration: BoxDecoration(
                    color: AppTheme.current.mutedText.withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
                Text(
                  "ATTACH CONTENT",
                  style: TextStyle(
                    color: AppTheme.current.accent,
                    fontSize: 11,
                    fontWeight: FontWeight.bold,
                    letterSpacing: 1.5,
                  ),
                ),
                const SizedBox(height: 24),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    _buildAttachmentItem(
                      icon: Icons.image_rounded,
                      color: AppTheme.current.accent,
                      label: "IMAGE",
                      onTap: () {
                        Navigator.pop(context);
                        _pickAndSendImage();
                      },
                    ),
                    _buildAttachmentItem(
                      icon: Icons.video_library_rounded,
                      color: Colors.purpleAccent,
                      label: "VIDEO",
                      onTap: () {
                        Navigator.pop(context);
                        _pickAndSendVideo();
                      },
                    ),
                    _buildAttachmentItem(
                      icon: Icons.insert_drive_file_rounded,
                      color: Colors.blueAccent,
                      label: "FILE",
                      onTap: () {
                        Navigator.pop(context);
                        _sendFile();
                      },
                    ),
                  ],
                ),
                const SizedBox(height: 20),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    _buildAttachmentItem(
                      icon: Icons.location_on_rounded,
                      color: Colors.redAccent,
                      label: "LOCATION",
                      onTap: () {
                        Navigator.pop(context);
                        _shareLocation();
                      },
                    ),
                    _buildAttachmentItem(
                      icon: Icons.poll_rounded,
                      color: Colors.orangeAccent,
                      label: "POLL",
                      onTap: () {
                        Navigator.pop(context);
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(content: Text("Polls are available in group chats")),
                        );
                      },
                    ),
                    _buildAttachmentItem(
                      icon: Icons.checklist_rounded,
                      color: Colors.tealAccent,
                      label: "LIST",
                      onTap: () {
                        Navigator.pop(context);
                        // TODO: Implement checklist
                      },
                    ),
                  ],
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  void _shareLocation() async {
    try {
      bool serviceEnabled = await Geolocator.isLocationServiceEnabled();
      if (!serviceEnabled) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Location services are disabled")));
        return;
      }

      LocationPermission permission = await Geolocator.checkPermission();
      if (permission == LocationPermission.denied) {
        permission = await Geolocator.requestPermission();
        if (permission == LocationPermission.denied) return;
      }
      
      if (permission == LocationPermission.deniedForever) return;

      // Open the map picker
      final result = await Navigator.push(
        context,
        MaterialPageRoute(builder: (context) => const LocationPickerScreen()),
      );

      // result is the selected LatLng from the picker
      if (result != null) {
        final text = "[LOCATION]:${result.latitude},${result.longitude}";
        _client.sendMessage(widget.peerId, text);
        _loadMessages();
      }
    } catch (_) {}
  }

  Widget _buildAttachmentItem({
    required IconData icon,
    required Color color,
    required String label,
    required VoidCallback onTap,
  }) {
    return GestureDetector(
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 56,
            height: 56,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.15),
              shape: BoxShape.circle,
              border: Border.all(color: color.withValues(alpha: 0.3), width: 1),
            ),
            child: Icon(icon, color: color, size: 24),
          ),
          const SizedBox(height: 8),
          Text(
            label,
            style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12),
          ),
        ],
      ),
    );
  }
  void _votePoll(String pollId, int optionIndex) {
    // Polls in 1:1 chats are read-only — voting only available in group chats
  }

  Widget _buildRecordingOverlay() {
    if (!_isRecording) return const SizedBox.shrink();
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 12),
      decoration: BoxDecoration(
        color: AppTheme.current.surface,
        border: Border(top: BorderSide(color: Colors.redAccent.withValues(alpha: 0.3), width: 1)),
      ),
      child: SafeArea(
        top: false,
        child: Row(
          children: [
            Container(
              width: 12,
              height: 12,
              decoration: const BoxDecoration(
                color: Colors.redAccent,
                shape: BoxShape.circle,
              ),
            ),
            const SizedBox(width: 12),
            Text(
              _formatRecordingDuration(_recordingDuration),
              style: const TextStyle(color: Colors.redAccent, fontSize: 16, fontFamily: 'monospace'),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                "Recording...",
                style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 13),
              ),
            ),
            IconButton(
              onPressed: _cancelRecording,
              icon: const Icon(Icons.delete_outline, color: Colors.redAccent),
              tooltip: 'Cancel',
            ),
            GestureDetector(
              onTap: _stopRecordingAndSend,
              child: Container(
                width: 48,
                height: 48,
                decoration: const BoxDecoration(
                  color: Colors.redAccent,
                  shape: BoxShape.circle,
                ),
                child: const Icon(Icons.send, color: Colors.white, size: 22),
              ),
            ),
          ],
        ),
      ),
    );
  }

  String _formatRecordingDuration(double seconds) {
    final s = seconds.toInt();
    final m = (s ~/ 60).toString().padLeft(2, '0');
    final sec = (s % 60).toString().padLeft(2, '0');
    return '$m:$sec';
  }

  Future<void> _startRecording() async {
    try {
      final hasPermission = await _audioRecorder.hasPermission();
      if (!hasPermission) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text("Microphone permission required for voice memos")),
          );
        }
        return;
      }

      await _audioRecorder.start(
        const RecordConfig(
          encoder: AudioEncoder.aacLc,
          bitRate: 128000,
          sampleRate: 44100,
        ),
        path: '${(await getTemporaryDirectory()).path}/recording_${DateTime.now().millisecondsSinceEpoch}.m4a',
      );

      setState(() {
        _isRecording = true;
        _recordingDuration = 0.0;
      });

      _recordingTimer = Timer.periodic(const Duration(seconds: 1), (_) {
        if (mounted) {
          setState(() => _recordingDuration += 1.0);
        }
      });
    } catch (e) {
      debugPrint("Error starting recording: $e");
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Failed to start recording: $e")),
        );
      }
    }
  }

  Future<void> _stopRecordingAndSend() async {
    _recordingTimer?.cancel();
    _recordingTimer = null;

    try {
      final path = await _audioRecorder.stop();
      if (path == null || path.isEmpty) {
        if (mounted) setState(() => _isRecording = false);
        return;
      }

      final duration = _recordingDuration.toInt();
      final filename = "voice_memo_${duration}s.m4a";

      // Move to a permanent location in app documents
      final appDir = await getApplicationDocumentsDirectory();
      final permanentPath = '${appDir.path}/$filename';
      final tempFile = File(path);
      if (await tempFile.exists()) {
        await tempFile.copy(permanentPath);
        await tempFile.delete();
      }

      if (mounted) {
        setState(() => _isRecording = false);
      }

      // Send as file transfer
      _client.sendFile(widget.peerId, permanentPath);
      _loadMessages();
    } catch (e) {
      debugPrint("Error stopping recording: $e");
      if (mounted) {
        setState(() => _isRecording = false);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Failed to save recording: $e")),
        );
      }
    }
  }

  Future<void> _cancelRecording() async {
    _recordingTimer?.cancel();
    _recordingTimer = null;

    try {
      final path = await _audioRecorder.stop();
      if (path != null && path.isNotEmpty) {
        final tempFile = File(path);
        if (await tempFile.exists()) await tempFile.delete();
      }
    } catch (_) {}

    if (mounted) {
      setState(() => _isRecording = false);
    }
  }

  Widget _buildRightActionButton() {
    if (_isRecording) {
      return IconButton(
        onPressed: _stopRecordingAndSend,
        icon: const Icon(Icons.send, color: Colors.redAccent),
      );
    }
    if (!_isInputEmpty || _editingMsgId != null) {
      return IconButton(
        onPressed: _sendMessage,
        icon: Icon(_editingMsgId != null ? Icons.check_circle_outline : Icons.send_rounded, color: AppTheme.current.accent),
      );
    }
    return IconButton(
      onPressed: _startRecording,
      icon: Icon(Icons.mic_none, color: AppTheme.current.accent),
    );
  }

  @override
  Widget build(BuildContext context) {
    final bool hasSelection = _selectedMsg != null;
    return Scaffold(
      appBar: hasSelection ? _buildSelectionToolbar() : AppBar(
        leadingWidth: 130,
        leading: Row(children: [IconButton(icon: const Icon(Icons.arrow_back), onPressed: () => Navigator.pop(context)), SovereignAvatar(radius: 20, prestigeTier: _peerTier, avatar: widget.avatarBase64 != null ? MemoryImage(base64Decode(widget.avatarBase64!)) : null)]),
        title: InkWell(
          onTap: _showInfo,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(_peerName ?? "Peer", style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold), overflow: TextOverflow.ellipsis, maxLines: 1),
              Row(
                children: [
                  Container(
                    width: 7,
                    height: 7,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: _status == "Direct P2P" || _status == "Relay Active" ? AppTheme.current.accent : Colors.redAccent.withValues(alpha: 0.5),
                    ),
                  ),
                  const SizedBox(width: 4),
                  Text(
                    _status == "Direct P2P" || _status == "Relay Active" ? "online" : "offline",
                    style: TextStyle(fontSize: 11, color: AppTheme.current.mutedText.withValues(alpha: 0.8), fontWeight: FontWeight.w400),
                  ),
                ],
              ),
            ],
          ),
        ),
        actions: [
          if (_isE2eeActive) Icon(Icons.lock_outline, color: AppTheme.current.accent, size: 18),
          IconButton(
            onPressed: _startCall,
            icon: Icon(Icons.videocam_rounded, color: AppTheme.current.accent),
            tooltip: 'Video Call',
          ),
          IconButton(onPressed: _showInfo, icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText.withValues(alpha: 0.7))),
          const SizedBox(width: 8)
        ],
      ),
      body: Stack(
        children: [
          SovereignWallpaper(),
          Column(
            children: [
              if (_isSyncing)
                Container(
                  padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                  color: AppTheme.current.accent.withValues(alpha: 0.1),
                  child: Row(
                    children: [
                      SizedBox(
                        width: 12,
                        height: 12,
                        child: CircularProgressIndicator(strokeWidth: 1.5, color: AppTheme.current.accent),
                      ),
                      SizedBox(width: 8),
                      Text("Catching up & syncing chat...", style: TextStyle(color: AppTheme.current.accent, fontSize: 11)),
                    ],
                  ),
                ),
              Expanded(
                child: _isLoading ? Center(child: CircularProgressIndicator(color: AppTheme.current.accent)) : ListView.builder(
                  controller: _scrollController,
                  itemCount: _displayMessages.length,
                  itemBuilder: (context, index) {
                    final msg = _displayMessages[index];
                    bool isMe = false;
                    DateTime msgDate = DateTime.now();
                    if (msg is MessageModel) {
                      isMe = msg.isMe;
                      msgDate = msg.timestamp;
                    } else if (msg is FileTransferProgress) {
                      isMe = msg.isOutgoing;
                      msgDate = msg.startDateTime;
                    } else if (msg is ImageGroupProgress) {
                      isMe = msg.isOutgoing;
                      msgDate = msg.startDateTime;
                    }
                    
                    bool showDateSeparator = false;
                    if (index == 0) {
                      showDateSeparator = true;
                    } else {
                      final prevMsg = _displayMessages[index - 1];
                      DateTime prevDate = DateTime.now();
                      if (prevMsg is MessageModel) {
                        prevDate = prevMsg.timestamp;
                      } else if (prevMsg is FileTransferProgress) {
                        prevDate = prevMsg.startTimeMs > 946684800000 ? DateTime.fromMillisecondsSinceEpoch(prevMsg.startTimeMs) : DateTime.now();
                      } else if (prevMsg is ImageGroupProgress) {
                        prevDate = prevMsg.startTimeMs > 946684800000 ? DateTime.fromMillisecondsSinceEpoch(prevMsg.startTimeMs) : DateTime.now();
                      }
                      if (msgDate.year != prevDate.year || msgDate.month != prevDate.month || msgDate.day != prevDate.day) {
                        showDateSeparator = true;
                      }
                    }

                    final avatarWidget = SovereignAvatar(radius: 21, prestigeTier: isMe ? _myTier : _peerTier, avatar: isMe ? (_myAvatar != null ? MemoryImage(base64Decode(_myAvatar!)) : null) : (widget.avatarBase64 != null ? MemoryImage(base64Decode(widget.avatarBase64!)) : null));
                    
                    final bubble = Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
                      child: GestureDetector(
                        onHorizontalDragUpdate: (details) { if (details.delta.dx > 8) setState(() => _replyingTo = msg); },
                        onLongPress: () {
                          setState(() {
                            _selectedMsg = (_selectedMsg == msg) ? null : msg;
                          });
                        },
                        onTap: () {
                          if (_selectedMsg != null) {
                            setState(() => _selectedMsg = null);
                          }
                        },
                        child: Row(
                          mainAxisAlignment: isMe ? MainAxisAlignment.end : MainAxisAlignment.start,
                          crossAxisAlignment: CrossAxisAlignment.end,
                          children: [if (!isMe) ...[avatarWidget, const SizedBox(width: 8)], Flexible(child: Column(crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start, children: [if (!isMe) Text(_peerName ?? 'mesh peer', style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold)), _buildBubbleContent(msg)])), if (isMe) ...[const SizedBox(width: 8), avatarWidget]],
                        ),
                      ),
                    );

                    if (showDateSeparator) {
                      final now = DateTime.now();
                      final today = DateTime(now.year, now.month, now.day);
                      final msgDay = DateTime(msgDate.year, msgDate.month, msgDate.day);
                      final diff = today.difference(msgDay).inDays;
                      
                      String dateText;
                      if (diff == 0) dateText = "TODAY";
                      else if (diff == 1) dateText = "YESTERDAY";
                      else if (diff < 7) {
                        // This week: show day name
                        final dayNames = ['MONDAY', 'TUESDAY', 'WEDNESDAY', 'THURSDAY', 'FRIDAY', 'SATURDAY', 'SUNDAY'];
                        dateText = dayNames[msgDate.weekday - 1];
                      } else {
                        dateText = DateFormat('dd MMM yy').format(msgDate).toUpperCase();
                      }

                      return Column(
                        children: [
                          Container(
                            margin: const EdgeInsets.symmetric(vertical: 16),
                            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
                            decoration: BoxDecoration(
                              color: AppTheme.current.surface,
                              borderRadius: BorderRadius.circular(12),
                              border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
                            ),
                            child: Text(
                              dateText,
                              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.1),
                            ),
                          ),
                          bubble,
                        ],
                      );
                    }
                    return bubble;
                  },
                ),
              ),
          if (_isRecording) _buildRecordingOverlay() else _buildInputArea(),
          if (_showPanel)
            StickerEmojiPanel(
              onEmojiSelect: (emoji) {
                final text = _messageController.text;
                final selection = _messageController.selection;
                if (selection.isValid) {
                  final newText = text.replaceRange(selection.start, selection.end, emoji);
                  _messageController.value = TextEditingValue(
                    text: newText,
                    selection: TextSelection.collapsed(offset: selection.start + emoji.length),
                  );
                } else {
                  _messageController.text += emoji;
                  _messageController.selection = TextSelection.collapsed(offset: _messageController.text.length);
                }
              },
              onStickerSelect: (name) {
                _sendSticker(name);
                setState(() => _showPanel = false);
              },
              onGifSelect: (url) {
                _sendGif(url);
                setState(() => _showPanel = false);
              },
            ),
        ],
      ),
      Positioned(top: 20, left: 0, right: 0, child: Center(child: RewardsHUD(relayedBytes: _relayedBytes.toInt(), solRewards: _solRewards))),
      // Elevated Messages mini tab
      Positioned(
        top: 60,
        right: 8,
        child: GestureDetector(
          onTap: () => _showElevatedMessages(context),
          child: Container(
            padding: EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            decoration: BoxDecoration(
              color: AppTheme.current.surface.withValues(alpha: 0.85),
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3)),
              boxShadow: [BoxShadow(color: Colors.black.withValues(alpha: 0.15), blurRadius: 8, offset: Offset(0, 2))],
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.push_pin, size: 14, color: AppTheme.current.accent),
                SizedBox(width: 4),
                Text(
                  _elevatedCount > 0 ? 'Elevated ($_elevatedCount)' : 'Elevated',
                  style: TextStyle(
                    color: _elevatedCount > 0 ? AppTheme.current.accent : AppTheme.current.mutedText,
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    ],
  ),
);
}

  Widget _buildInputArea() {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (_replyingTo != null) Container(padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)), left: BorderSide(color: AppTheme.current.accent, width: 4))), child: Row(children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text("Replying to ${_replyingTo is MessageModel ? (_replyingTo.isMe ? 'me' : (_peerName ?? 'peer')) : 'File'}", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold)), Text(_replyingTo is MessageModel ? _replyingTo.content : (_replyingTo as FileTransferProgress).filename, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12))])), IconButton(icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.7)), onPressed: () => setState(() => _replyingTo = null))])),
        if (_editingMsgId != null) Container(padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)), left: BorderSide(color: Colors.orangeAccent, width: 4))), child: Row(children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text("EDITING MESSAGE", style: TextStyle(color: Colors.orangeAccent, fontSize: 10, fontWeight: FontWeight.bold)), Text(_messageController.text, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12))])), IconButton(icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.7)), onPressed: () { setState(() { _editingMsgId = null; _messageController.clear(); }); })])),
        Container(padding: EdgeInsets.all(20), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1), width: 0.5))), child: SafeArea(child: Row(children: [IconButton(onPressed: _showAttachmentOptions, icon: Icon(Icons.attach_file_rounded, color: AppTheme.current.accent)), IconButton(onPressed: () => setState(() => _showPanel = !_showPanel), icon: Icon(_showPanel ? Icons.keyboard_rounded : Icons.sentiment_satisfied_alt_rounded, color: AppTheme.current.accent)), SizedBox(width: 8), Expanded(child: _isRecording ? SizedBox.shrink() : TextField(controller: _messageController, style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'), decoration: InputDecoration(hintText: _replyingTo != null ? "WRITE YOUR REPLY..." : (_editingMsgId != null ? "EDITING..." : "ENTER ENCRYPTED PAYLOAD..."), hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 12), filled: true, fillColor: AppTheme.current.text.withValues(alpha: 0.05), border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none), contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 12)), onSubmitted: (_) => _sendMessage())), SizedBox(width: 12), _buildRightActionButton()]))),
      ],
    );
  }
}
