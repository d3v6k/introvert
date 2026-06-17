import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:image_picker/image_picker.dart';
import 'package:geolocator/geolocator.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uuid/uuid.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:image/image.dart' as img;
import '../src/native/introvert_client.dart';
import '../src/ui/widgets/file_transfer_bubble.dart';
import '../src/ui/widgets/image_stack_bubble.dart';
import '../blueprint_ui.dart';
import 'chat_features.dart';
import 'media_gallery_viewer.dart';
import 'package:record/record.dart';
import 'package:intl/intl.dart';
import '../theme/app_theme.dart';
import 'group_call_screen.dart';
import '../src/services/group_call_service.dart';
import 'location_picker_screen.dart';


class GroupChatScreen extends StatefulWidget {
  final String groupId;
  final String groupName;

  const GroupChatScreen({
    required this.groupId,
    required this.groupName,
    super.key,
  });

  @override
  State<GroupChatScreen> createState() => _GroupChatScreenState();
}

class _GroupChatScreenState extends State<GroupChatScreen> {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  
  final Map<String, FileTransferProgress> _groupTransfers = {};
  StreamSubscription<FileTransferProgress>? _transferSubscription;
  List<dynamic> _messages = [];
  List<dynamic> _members = [];
  StreamSubscription<NetworkEvent>? _networkSubscription;
  final Map<String, String> _contactNames = {};
  final Map<String, String> _contactAvatars = {}; // peer_id -> base64 avatar
  String? _myAvatar;
  dynamic _replyingTo;
  String? _editingMsgId;
  bool _isAdmin = false;

  bool _isInputEmpty = true;
  bool _isSyncing = false;
  bool _showPanel = false;
  
  // Active List state
  String _activeListTitle = "";
  List<Map<String, dynamic>> _activeListItems = [];
  String _activeListCreatorId = "";
  
  // Poll state
  final Map<String, Map<String, List<String>>> _polls = {};
  final Map<String, String> _pollQuestions = {};
  final Map<String, List<String>> _pollOptions = {};
  
  // Recording state
  bool _isRecording = false;
  final AudioRecorder _audioRecorder = AudioRecorder();
  
  String? _docDirPath;
  final Set<String> _polledPeers = {};

  // Active group call state
  String? _activeCallId;
  String? _activeCallCallerId;
  int _activeCallMediaType = 2;
  List<String> _activeCallMembers = [];
  Timer? _callExpiryTimer;

  void _loadContactNames() {
    try {
      final profile = _client.getProfile();
      _myAvatar = profile['avatar'];

      final contacts = _client.getContacts();
      for (var c in contacts) {
        final pid = c['peer_id']?.toString() ?? '';
        final alias = c['alias']?.toString() ?? '';
        final globalName = c['global_name']?.toString() ?? '';
        final avatar = c['avatar']?.toString() ?? '';
        if (pid.isNotEmpty) {
          final displayName = alias.isNotEmpty ? alias : (globalName.isNotEmpty ? globalName : pid);
          _contactNames[pid] = displayName;
          if (avatar.isNotEmpty) {
            _contactAvatars[pid] = avatar;
          }
        }
      }

      final allGroups = _client.getAllGroups();
      final currentGroup = allGroups.firstWhere(
        (g) => g is List && g.isNotEmpty && g[0] == widget.groupId,
        orElse: () => null,
      );
      if (currentGroup != null && currentGroup is List && currentGroup.length >= 3) {
        try {
          final List<dynamic> members = json.decode(currentGroup[2] as String);
          _members = members;
          final localId = _client.localPeerId;
          for (var m in members) {
            if (m is Map && m['peer_id'] != null) {
              final pid = m['peer_id'].toString();
              
              final alias = m['alias']?.toString() ?? '';
              final avatar = m['avatar']?.toString() ?? '';
              final role = m['role']?.toString() ?? '';

              // Poll profile for graceful background updates if missing or first time
              if (pid != localId && !_polledPeers.contains(pid)) {
                if (alias.isEmpty || avatar.isEmpty || alias == pid) {
                   _client.pollPeerProfile(pid);
                   _polledPeers.add(pid);
                }
              }
              
              if (alias.isNotEmpty) {
                _contactNames[pid] = alias;
              }
              if (avatar.isNotEmpty) {
                _contactAvatars[pid] = avatar;
              }
              if (pid == localId) {
                _isAdmin = role == "Creator" || role == "Admin";
              }
            }
          }
        } catch (_) {}
      }
    } catch (e) {
      debugPrint("Error loading contact names: $e");
    }
  }

  @override
  void initState() {
    super.initState();
    _messageController.addListener(() {
      final val = _messageController.text.trim().isEmpty;
      if (val != _isInputEmpty) {
        setState(() {
          _isInputEmpty = val;
        });
      }
    });
    _loadActiveList();
    Future.microtask(() async {
      final dir = await getApplicationDocumentsDirectory();
      if (mounted) {
        _docDirPath = dir.path;
        _loadContactNames();
        _markMessagesAsRead();
        _loadMessages();
      }
    });
    _startListener();
    
    // Sync missed messages from all group members
    Future.microtask(() async {
      if (!mounted) return;
      final memberIds = _members.map((m) => m['peer_id']?.toString() ?? '').where((id) => id.isNotEmpty && id != _client.localPeerId).toList();
      if (memberIds.isNotEmpty) {
        setState(() => _isSyncing = true);
        // Round 1: Sync with all connected members
        for (final memberId in memberIds) {
          _client.syncChatMessages(memberId, widget.groupId, true);
        }
        // Round 2: After relay propagation, sync again to catch messages relayed by others
        Future.delayed(Duration(seconds: 2), () {
          if (!mounted) return;
          for (final memberId in memberIds) {
            _client.syncChatMessages(memberId, widget.groupId, true);
          }
          Future.delayed(Duration(seconds: 2), () {
            if (mounted) {
              setState(() => _isSyncing = false);
              _loadMessages();
            }
          });
        });
      }
    });
    
    _transferSubscription = _client.transferStream.listen((progress) {
      if (mounted) {
        if (progress.groupId != widget.groupId) return; // Leakage Fix: Skip 1-on-1 or other group transfers
        setState(() {
          _groupTransfers[progress.transferId] = progress;
        });
        _loadMessages();
      }
    });
  }

  @override
  void dispose() {
    _networkSubscription?.cancel();
    _transferSubscription?.cancel();
    _audioRecorder.dispose();
    _callExpiryTimer?.cancel();
    super.dispose();
  }

  Future<void> _loadActiveList() async {
    final prefs = await SharedPreferences.getInstance();
    final key = "active_list_${widget.groupId}";
    final data = prefs.getString(key);
    if (data != null) {
      try {
        final decoded = json.decode(data);
        setState(() {
          _activeListTitle = decoded["title"] ?? "";
          _activeListItems = List<Map<String, dynamic>>.from(
            (decoded["items"] as List).map((i) => Map<String, dynamic>.from(i))
          );
          _activeListCreatorId = decoded["creator_id"]?.toString() ?? "";
        });
      } catch (e) {
        debugPrint("Error loading active list: $e");
      }
    }
  }

  Future<void> _saveAndSendActiveList(String title, List<Map<String, dynamic>> items) async {
    final prefs = await SharedPreferences.getInstance();
    final key = "active_list_${widget.groupId}";
    final String creator = _activeListCreatorId.isNotEmpty ? _activeListCreatorId : (_client.localPeerId ?? "");
    final Map<String, dynamic> data = {
      "title": title,
      "items": items,
      "creator_id": creator,
    };
    await prefs.setString(key, json.encode(data));
    setState(() {
      _activeListTitle = title;
      _activeListItems = items;
      _activeListCreatorId = creator;
    });

    final payload = "[ACTIVE_LIST]:${json.encode(data)}";
    try {
      _client.sendGroupMessage(widget.groupId, payload);
    } catch (e) {
      debugPrint("Failed to send group active list: $e");
    }
  }

  bool _handleCustomProtocolMessage(String content, bool isMe, DateTime ts) {
    if (content.startsWith("[GROUP_CALL_INVITE]:")) {
      try {
        final jsonStr = content.substring(19);
        final decoded = json.decode(jsonStr);
        final callId = decoded['call_id']?.toString();
        final callerId = decoded['caller_id']?.toString();
        final mediaType = decoded['media_type'] as int? ?? 2;
        final members = List<String>.from(decoded['members'] ?? []);
        
        if (callId != null && callerId != null && mounted) {
          // Don't show if we're already in this call or if we're the caller
          if (GroupCallService.instance.callId != callId && callerId != _client.localPeerId) {
            setState(() {
              _activeCallId = callId;
              _activeCallCallerId = callerId;
              _activeCallMediaType = mediaType;
              _activeCallMembers = members;
            });
            // Auto-expire after 2 minutes
            _callExpiryTimer?.cancel();
            _callExpiryTimer = Timer(const Duration(minutes: 2), () {
              if (mounted) {
                setState(() {
                  _activeCallId = null;
                  _activeCallCallerId = null;
                  _activeCallMembers = [];
                });
              }
            });
          }
        }
      } catch (e) {
        debugPrint("Error parsing group call invite: $e");
      }
      return true; // Don't show as regular message
    }
    if (content.startsWith("[GROUP_CALL_JOIN]:")) {
      try {
        final jsonStr = content.substring(17);
        final decoded = json.decode(jsonStr);
        final peerId = decoded['peer_id']?.toString();
        final callId = decoded['call_id']?.toString();
        if (callId == _activeCallId && peerId != null && !_activeCallMembers.contains(peerId)) {
          setState(() {
            _activeCallMembers.add(peerId);
          });
        }
      } catch (_) {}
      return true;
    }
    if (content.startsWith("[GROUP_CALL_LEAVE]:")) {
      try {
        final jsonStr = content.substring(18);
        final decoded = json.decode(jsonStr);
        final peerId = decoded['peer_id']?.toString();
        final callId = decoded['call_id']?.toString();
        if (callId == _activeCallId && peerId != null) {
          setState(() {
            _activeCallMembers.remove(peerId);
            if (_activeCallMembers.isEmpty) {
              _activeCallId = null;
              _activeCallCallerId = null;
            }
          });
        }
      } catch (_) {}
      return true;
    }
    if (content.startsWith("[ACTIVE_LIST]:")) {
      try {
        final jsonStr = content.substring(14);
        final decoded = json.decode(jsonStr);
        _activeListTitle = decoded["title"] ?? "";
        _activeListItems = List<Map<String, dynamic>>.from(
          (decoded["items"] as List).map((i) => Map<String, dynamic>.from(i))
        );
        _activeListCreatorId = decoded["creator_id"]?.toString() ?? "";
        SharedPreferences.getInstance().then((prefs) {
          prefs.setString("active_list_${widget.groupId}", json.encode(decoded));
        });
      } catch (e) {
        debugPrint("Error parsing active list: $e");
      }
      return true;
    }
    if (content.startsWith("[POLL_CREATE]:")) {
      try {
        final jsonStr = content.substring(14);
        final decoded = json.decode(jsonStr);
        final pollId = decoded["poll_id"];
        final question = decoded["question"];
        final options = List<String>.from(decoded["options"]);
        _pollQuestions[pollId] = question;
        _pollOptions[pollId] = options;
        _polls.putIfAbsent(pollId, () => <String, List<String>>{ for (var opt in options) opt: <String>[] });
      } catch (e) {
        debugPrint("Error parsing poll create: $e");
      }
      return false;
    }
    if (content.startsWith("[POLL_VOTE]:")) {
      try {
        final jsonStr = content.substring(12);
        final decoded = json.decode(jsonStr);
        final pollId = decoded["poll_id"];
        final optionIndex = decoded["option_index"] as int;
        final voter = decoded["voter"];
        
        final opts = _pollOptions[pollId];
        if (opts != null && optionIndex < opts.length) {
          final optionName = opts[optionIndex];
          _polls.putIfAbsent(pollId, () => <String, List<String>>{ for (var opt in opts) opt: <String>[] });
          for (var opt in opts) {
            _polls[pollId]![opt]!.remove(voter);
          }
          _polls[pollId]![optionName]!.add(voter);
        }
      } catch (e) {
        debugPrint("Error parsing poll vote: $e");
      }
      return true;
    }
    return false;
  }

  void _markMessagesAsRead() {
    _client.updateGroupMessageStatus(widget.groupId, 0);
    // Send read receipts for each unread incoming group message
    try {
      final msgs = _client.getGroupMessages(widget.groupId);
      for (var m in msgs) {
        if (m == null || m.length < 5) continue;
        final senderId = m[0]?.toString() ?? '';
        final msgId = m[4]?.toString() ?? '';
        final isMe = senderId == _client.localPeerId;
        if (!isMe && msgId.isNotEmpty) {
          _client.sendAcknowledgement(senderId, msgId, 2);
        }
      }
    } catch (e) {
      debugPrint("Error sending group read receipts: $e");
    }
  }

  void _loadMessages() {
    if (!mounted) return;
    _markMessagesAsRead();
    final msgs = _client.getGroupMessages(widget.groupId);
    setState(() {
      final List<dynamic> processed = [];
      for (var m in msgs) {
        if (m == null || m.length < 5) continue;
        final senderId = m[0]?.toString() ?? '';
        final senderName = m[1]?.toString() ?? '';
        final content = m[2]?.toString() ?? '';
        final timestamp = m[3]?.toString() ?? '';
        final senderAvatar = m.length > 6 ? m[6]?.toString() : null;
        final isMe = senderId == _client.localPeerId;

        if (senderId.isNotEmpty && senderName.isNotEmpty) {
           _contactNames[senderId] = senderName;
        }
        if (senderId.isNotEmpty && senderAvatar != null && senderAvatar.isNotEmpty) {
           _contactAvatars[senderId] = senderAvatar;
        }
        
        DateTime ts = DateTime.tryParse(timestamp) ?? DateTime.now();
        
        if (_handleCustomProtocolMessage(content, isMe, ts)) {
          continue;
        }
        
        if (content.startsWith("[FILE]:")) {
          try {
            final jsonStr = content.substring(7);
            final meta = json.decode(jsonStr);
            final tid = meta['transfer_id']?.toString() ?? '';
            
            if (tid.isEmpty) continue;
            
            if (_groupTransfers.containsKey(tid)) {
              final active = _groupTransfers[tid]!;
              processed.add(active);
              continue;
            }
            
            final isOutgoing = senderId == _client.localPeerId;
            String? localPath = _client.resolveSandboxPath(meta['local_path']?.toString());
            bool exists = false;
            final fileHash = meta['file_hash']?.toString() ?? '';

            if (localPath != null && localPath.isNotEmpty) {
              exists = File(localPath).existsSync();
            }

            if (!exists && !isOutgoing) {
              try {
                final driveFiles = _client.driveGetAll();
                final driveFile = driveFiles.firstWhere((f) => f['file_hash'] == fileHash, orElse: () => null);
                if (driveFile != null) {
                  localPath = _client.resolveSandboxPath(driveFile['local_path']?.toString());
                  if (localPath != null && localPath.isNotEmpty) {
                    exists = File(localPath).existsSync();
                  }
                }
              } catch (_) {}
            }

            if (!exists && _docDirPath != null) {
              final filename = meta['filename']?.toString() ?? 'unknown';
              final safeFilename = filename.replaceAll(RegExp(r'[^a-zA-Z0-9\.\-_]'), '_');
              final potentialPath = '$_docDirPath/introvert_$safeFilename';
              if (File(potentialPath).existsSync()) {
                exists = true;
                localPath = potentialPath;
              }
            }

            final filename = meta['filename']?.toString() ?? 'unknown';
            final mimeType = meta['mime_type']?.toString() ?? 'application/octet-stream';

            if (!isOutgoing && !exists) {
                final totalSize = (meta['total_size'] as num?)?.toInt() ?? 0;
                _client.startPull(senderId, tid, filename, mimeType, fileHash, totalSize, true, widget.groupId);
            }
            
            final progressObj = FileTransferProgress(
              transferId: tid,
              peerId: senderId,
              filename: filename,
              mimeType: mimeType,
              fileHash: fileHash,
              progress: exists ? 1.0 : 0.0,
              speedBps: 0.0,
              isComplete: exists,
              isVerified: exists,
              isOutgoing: isOutgoing,
              isCancelled: false,
              localPath: localPath,
              startTimeMs: ts.millisecondsSinceEpoch,
              isWaitingForDownload: (!isOutgoing && !exists),
              thumbnail: meta['thumbnail']?.toString(),
            );
            _groupTransfers[tid] = progressObj;
            processed.add(progressObj);
            continue;
          } catch (e) {
            debugPrint("Error parsing group file message: $e");
          }
        }
        
        processed.add(m);
      }
      _messages = processed;
    });
    _scrollToBottom();
  }

  void _scrollToMessage(String? msgId) {
    if (msgId == null) return;
    final index = _messages.indexWhere((m) {
      if (m is List && m.length > 4) return m[4] == msgId;
      if (m is FileTransferProgress) return m.transferId == msgId;
      return false;
    });

    if (index != -1) {
      _scrollController.animateTo(
        index * 80.0,
        duration: const Duration(milliseconds: 500),
        curve: Curves.easeInOut,
      );
    }
  }

  Widget _buildBubbleContent(dynamic msg, [String? replyTo]) {
    bool isMe = false;
    DateTime ts = DateTime.now();
    String content = "";
    String? msgId;

    if (msg is FileTransferProgress) {
      isMe = msg.peerId == _client.localPeerId;
      ts = DateTime.fromMillisecondsSinceEpoch(msg.startTimeMs);
      msgId = msg.transferId;
      
      bool exists = false;
      String? localPath = msg.localPath;
      
      // VITAL FIX: If the message thinks the file is missing, check the Sovereign Drive (where it's auto-organized)
      if (localPath == null || !File(localPath).existsSync()) {
        final driveInfo = _client.driveGetByHash(msg.fileHash);
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

      final updatedProgress = FileTransferProgress(
        transferId: msg.transferId,
        peerId: msg.peerId,
        filename: msg.filename,
        mimeType: msg.mimeType,
        fileHash: msg.fileHash,
        progress: exists ? 1.0 : msg.progress,
        speedBps: msg.speedBps,
        isComplete: exists || msg.isComplete,
        isVerified: exists || msg.isVerified,
        isOutgoing: isMe,
        isCancelled: msg.isCancelled,
        localPath: localPath,
        startTimeMs: msg.startTimeMs,
        isWaitingForDownload: (!isMe && !exists && msg.isWaitingForDownload),
        thumbnail: msg.thumbnail,
      );

      if (msg.filename.startsWith("voice_memo_")) {
        return VoiceMemoBubble(
          filename: msg.filename,
          isMe: isMe,
          timestamp: ts,
          localPath: localPath ?? '',
          reactions: _client.getMessageReactions(msgId),
        );
      }
      if (msg.filename.startsWith("sticker_")) {
        return StickerBubble(
          name: localPath ?? msg.filename,
          isMe: isMe,
          timestamp: ts,
          reactions: _client.getMessageReactions(msgId),
        );
      }
      return FileTransferBubble(
        progress: updatedProgress,
        isMe: isMe,
        reactions: _client.getMessageReactions(msgId),
        allMessages: _messages,
        onTap: () {
          if (!updatedProgress.isComplete && !updatedProgress.isVerified && !isMe) {
            final msgs = _client.getGroupMessages(widget.groupId);
            final groupMsg = msgs.firstWhere(
              (m) => m[2].toString().startsWith("[FILE]:") && m[2].toString().contains(msg.transferId),
              orElse: () => null,
            );
            if (groupMsg != null) {
              try {
                final meta = json.decode(groupMsg[2].substring(7));
                meta['file_hash']?.toString();
                meta['total_size'];
              } catch (_) {}
            }
          }
        },
      );
    } else if (msg is ImageGroupProgress) {
      final isMe = msg.images.first.peerId == _client.localPeerId;
      final msgId = msg.images.first.transferId;
      return ImageStackBubble(
        group: msg,
        isMe: isMe,
        reactions: _client.getMessageReactions(msgId),
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
    } else {
      final senderId = msg[0]?.toString() ?? '';
      final msgIdVal = msg.length > 4 ? msg[4].toString() : '';
      content = msg[2]?.toString() ?? '';
      final timestampStr = msg[3]?.toString() ?? '';
      isMe = senderId == _client.localPeerId;
      ts = _parseTimestamp(timestampStr);

      if (content.startsWith("[STICKER]:")) {
        return StickerBubble(
          name: content.substring(10),
          isMe: isMe,
          timestamp: ts,
          reactions: msgIdVal.isNotEmpty ? _client.getMessageReactions(msgIdVal) : null,
          msgId: msgIdVal,
          onReactionTap: msgIdVal.isNotEmpty ? () => _showReactionDetails(msgIdVal, _client.getMessageReactions(msgIdVal)) : null,
        );
      }
      if (content.startsWith("[GIF]:")) {
        return GifBubble(
          url: content.substring(6),
          isMe: isMe,
          timestamp: ts,
          reactions: msgIdVal.isNotEmpty ? _client.getMessageReactions(msgIdVal) : null,
          msgId: msgIdVal,
          onReactionTap: msgIdVal.isNotEmpty ? () => _showReactionDetails(msgIdVal, _client.getMessageReactions(msgIdVal)) : null,
        );
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
              isMe: isMe,
              timestamp: ts,
              reactions: msgIdVal.isNotEmpty ? _client.getMessageReactions(msgIdVal) : null,
              msgId: msgIdVal,
              onReactionTap: msgIdVal.isNotEmpty ? () => _showReactionDetails(msgIdVal, _client.getMessageReactions(msgIdVal)) : null,
            );
          }
        }
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
            isMe: isMe,
            timestamp: ts,
            localPeerId: _client.localPeerId ?? '',
            onVote: (idx) => _votePoll(pollId, idx),
            reactions: msgIdVal.isNotEmpty ? _client.getMessageReactions(msgIdVal) : null,
            msgId: msgIdVal,
            onReactionTap: msgIdVal.isNotEmpty ? () => _showReactionDetails(msgIdVal, _client.getMessageReactions(msgIdVal)) : null,
          );
        } catch (_) {}
      }

      dynamic replyTarget;
      ImageProvider? replyAv;
      if (replyTo != null) {
        try {
          replyTarget = _messages.firstWhere((m) => (m is FileTransferProgress && m.transferId == replyTo) || (m is List && m.length > 4 && m[4] == replyTo), orElse: () => null);
          if (replyTarget != null) {
            String? rPid;
            if (replyTarget is FileTransferProgress) rPid = replyTarget.peerId;
            if (replyTarget is List) rPid = replyTarget[0]?.toString();
            
            if (rPid != null) {
              if (rPid == _client.localPeerId) {
                if (_myAvatar != null) replyAv = MemoryImage(base64Decode(_myAvatar!));
              } else {
                final avStr = _contactAvatars[rPid];
                if (avStr != null) replyAv = MemoryImage(base64Decode(avStr));
              }
            }
          }
        } catch (_) {}
      }

      final reactions = msgIdVal.isNotEmpty ? _client.getMessageReactions(msgIdVal) : null;

      return GlassmorphicBubble(
        content: content,
        isMe: isMe,
        timestamp: ts,
        status: 1,
        replyTo: replyTarget,
        replyAvatar: replyAv,
        onReplyTap: () => _scrollToMessage(replyTo),
        reactions: reactions,
        msgId: msgIdVal,
        onReactionTap: reactions != null ? () => _showReactionDetails(msgIdVal, reactions) : null,
      );
    }
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
                  name = _contactNames[peerId] ?? peerId;
                }

                return Material(color: Colors.transparent, child: ListTile(
                  leading: SovereignAvatar(
                    radius: 24, 
                    avatar: isMe ? (_myAvatar != null ? MemoryImage(base64Decode(_myAvatar!)) : null) : (_contactAvatars[peerId] != null ? MemoryImage(base64Decode(_contactAvatars[peerId]!)) : null),
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

  void _showMessageActions(dynamic msg) {
    String content = "";
    String? msgId;
    bool isMe = false;
    DateTime ts = DateTime.now();

    if (msg is List && msg.length > 4) {
      msgId = msg[4].toString();
      content = msg[2].toString();
      isMe = msg[0] == _client.localPeerId;
      ts = DateTime.tryParse(msg[3].toString()) ?? DateTime.now();
    } else if (msg is FileTransferProgress) {
      content = "[FILE]:${msg.localPath ?? ''}";
      msgId = msg.transferId;
      isMe = msg.isOutgoing;
      ts = DateTime.fromMillisecondsSinceEpoch(msg.startTimeMs);
    }

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          SizedBox(height: 16),
          if (msgId != null)
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                ...["👍", "❤️", "😂", "😮", "😢", "🙏"].map((emoji) => 
                  GestureDetector(
                    onTap: () {
                    if (msgId != null) _client.sendReaction(widget.groupId, msgId, emoji, true);
                      Navigator.pop(context);
                      _loadMessages();
                    },
                    child: Text(emoji, style: TextStyle(fontSize: 24)),
                  )
                ).toList(),
                GestureDetector(
                  onTap: () {
                    Navigator.pop(context);
                    _showFullEmojiPicker(msgId!);
                  },
                  child: Container(
                    padding: EdgeInsets.all(4),
                    decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), shape: BoxShape.circle),
                    child: Icon(Icons.add, color: AppTheme.current.accent, size: 20),
                  ),
                ),
              ],
            ),
          SizedBox(height: 16),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1), height: 1),
          Material(color: Colors.transparent, child: ListTile(
            leading: Icon(Icons.reply, color: AppTheme.current.accent),
            title: Text("Reply", style: TextStyle(color: AppTheme.current.text)),
            onTap: () {
              Navigator.pop(context);
              setState(() => _replyingTo = msg);
            },
          )),
          Material(color: Colors.transparent, child: ListTile(
            leading: Icon(Icons.copy, color: AppTheme.current.accent),
            title: Text("Copy", style: TextStyle(color: AppTheme.current.text)),
            onTap: () {
              Navigator.pop(context);
              Clipboard.setData(ClipboardData(text: content));
              ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Copied to clipboard")));
            },
          )),
          Material(color: Colors.transparent, child: ListTile(
            leading: Icon(Icons.forward, color: AppTheme.current.accent),
            title: Text("Forward", style: TextStyle(color: AppTheme.current.text)),
            onTap: () {
              Navigator.pop(context);
              _showForwardDialog(content);
            },
          )),
          if (isMe && DateTime.now().difference(ts).inSeconds <= 60 && msg is! FileTransferProgress)
            Material(color: Colors.transparent, child: ListTile(
              leading: Icon(Icons.edit, color: AppTheme.current.accent),
              title: Text("Edit", style: TextStyle(color: AppTheme.current.text)),
              onTap: () {
                Navigator.pop(context);
                setState(() {
                  _editingMsgId = msgId;
                  _messageController.text = content;
                });
              },
            )),
          if (isMe || _isAdmin)
            Material(color: Colors.transparent, child: ListTile(
              leading: Icon(Icons.delete, color: Colors.redAccent),
              title: Text("Delete", style: TextStyle(color: Colors.redAccent)),
              onTap: () async {
                Navigator.pop(context);
                final confirm = await showDialog<bool>(
                  context: context,
                  builder: (ctx) => AlertDialog(
                    backgroundColor: AppTheme.current.surface,
                    title: Text("Delete Message?", style: TextStyle(color: Colors.redAccent)),
                    content: Text("This will delete the message for everyone in the mesh room.", style: TextStyle(color: AppTheme.current.text)),
                    actions: [
                      TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text("CANCEL")),
                      TextButton(onPressed: () => Navigator.pop(ctx, true), child: Text("DELETE", style: TextStyle(color: Colors.redAccent))),
                    ],
                  ),
                );
                  if (confirm == true && msgId != null) {
                    final isAdminDeletingOther = _isAdmin && !isMe;
                    _client.deleteMessage(widget.groupId, msgId, true, deletedByAdmin: isAdminDeletingOther);
                    setState(() {
                      if (isAdminDeletingOther) {
                         // Don't remove locally yet, let the refresh show the placeholder
                         _loadMessages();
                      } else {
                        _messages.removeWhere((m) {
                          if (m is FileTransferProgress) return m.transferId == msgId;
                          return (m is List && m.length > 4 && m[4] == msgId);
                        });
                      }
                    });
                  }
              },
            )),
          SizedBox(height: 24),
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
          SizedBox(height: 12),
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
                if (index >= emojis.length) return SizedBox.shrink();
                final emoji = emojis[index];
                return GestureDetector(
                  onTap: () {
                    _client.sendReaction(widget.groupId, msgId, emoji, true);
                    Navigator.pop(context);
                    _loadMessages();
                  },
                  child: Center(child: Text(emoji, style: TextStyle(fontSize: 24))),
                );
              },
            ),
          ),
        ],
      ),
    );
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
                      leading: SovereignAvatar(radius: 27, avatar: c['avatar'] != null ? MemoryImage(base64Decode(c['avatar'])) : null),
                      title: Text(c['alias'] ?? c['peer_id'], style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                      onTap: () {
                        if (content.startsWith("[FILE]:")) {
                           final path = _extractLocalPath(content);
                           if (path.isNotEmpty && File(path).existsSync()) {
                             _client.sendFile(c['peer_id'], path); 
                             Navigator.pop(ctx);
                             ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Message forwarded")));
                           } else {
                             ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Error: File not found locally")));
                           }
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
                        if (content.startsWith("[FILE]:")) {
                           final path = _extractLocalPath(content);
                           if (path.isNotEmpty && File(path).existsSync()) {
                             _client.sendFile("", path, g[0]);
                             Navigator.pop(ctx);
                             ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Message forwarded to group")));
                           } else {
                             ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Error: File not found locally")));
                           }
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

  void _startListener() {
    _networkSubscription = _client.networkStream.listen((event) {
      if (event.type == 21 || event.type == 23) {
         _loadMessages();
      } else if (event.type == 12) {
        if (!mounted) return;
        try {
          final progress = FileTransferProgress.fromJson(json.decode(utf8.decode(event.data)));
          // LEAKAGE FIX: Only process events for THIS group. Reject direct (groupId=null) and
          // other-group transfers. Mirrors the guard in _transferSubscription.
          if (progress.groupId != widget.groupId) return;
          setState(() {
            final idx = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
            if (idx != -1) {
              final existing = _messages[idx] as FileTransferProgress;
              _messages[idx] = FileTransferProgress(
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
            } else {
              // Check if we already have a manifest for this
              if (!_messages.any((m) => (m is List && m.length > 2 && m[2].toString().contains(progress.transferId)))) {
                 _messages.add(progress);
                 _scrollToBottom();
              }
            }
          });
        } catch (_) {}
      } else if (event.type == 35 || event.type == 37 || event.type == 38) {
        // Event 35: Message Reaction, 37: Deleted, 38: Edited
        if (mounted) setState(() {});
        _loadMessages();
      } else if (event.type == 22) {
        _loadContactNames();
        _loadMessages();
      } else if (event.type == 25) {
        if (!mounted || event.data.isEmpty) return;
        int offset = 0;
        final pidLen = event.data[offset++];
        offset += pidLen;

        // Refresh local state and trigger rebuild
        setState(() {
           _loadContactNames();
        });
      }

    });
  }

  void _sendMessage() {
    final text = _messageController.text.trim();
    if (text.isEmpty) return;

    if (_editingMsgId != null) {
      try {
        _client.editMessage(widget.groupId, _editingMsgId!, text, true);
        setState(() {
          final idx = _messages.indexWhere((m) {
            if (m is List && m.length > 4) return m[4] == _editingMsgId;
            return false;
          });
          if (idx != -1) {
             _messages[idx][2] = text;
          }
          _messageController.clear();
          _editingMsgId = null;
        });
      } catch (e) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Edit failed: $e")));
      }
      return;
    }

    final replyToId = _replyingTo is FileTransferProgress ? _replyingTo.transferId : (_replyingTo is List && _replyingTo.length > 4 ? _replyingTo[4].toString() : null);
    final localMsg = [_client.localPeerId, "me", text, DateTime.now().toUtc().toIso8601String() + "Z", const Uuid().v4(), replyToId];
    setState(() {
      _messages.add(localMsg);
      _messageController.clear();
      _replyingTo = null;
    });
    _scrollToBottom();
    _client.sendGroupMessage(widget.groupId, text, replyToId);
  }

  void _sendSticker(String name) {
    final payload = "[STICKER]:$name";
    final localMsg = [_client.localPeerId, "me", payload, DateTime.now().toUtc().toIso8601String() + "Z", const Uuid().v4(), null];
    setState(() {
      _messages.add(localMsg);
    });
    _scrollToBottom();
    _client.sendGroupMessage(widget.groupId, payload);
  }

  void _sendGif(String url) {
    final payload = "[GIF]:$url";
    final localMsg = [_client.localPeerId, "me", payload, DateTime.now().toUtc().toIso8601String() + "Z", const Uuid().v4(), null];
    setState(() {
      _messages.add(localMsg);
    });
    _scrollToBottom();
    _client.sendGroupMessage(widget.groupId, payload);
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(_scrollController.position.maxScrollExtent, duration: const Duration(milliseconds: 300), curve: Curves.easeOut);
      }
    });
  }

  void _syncContactDetails() {
    final memberIds = _members.map((m) => m['peer_id']?.toString() ?? '').where((id) => id.isNotEmpty && id != _client.localPeerId).toList();
    for (final memberId in memberIds) {
      _client.pollPeerProfile(memberId);
      _client.syncChatMessages(memberId, widget.groupId, true);
    }
    setState(() => _isSyncing = true);
    Future.delayed(Duration(seconds: 3), () {
      if (mounted) setState(() => _isSyncing = false);
    });
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text("Syncing ${memberIds.length} contact(s) & messages...", style: TextStyle(color: AppTheme.current.accent)),
        backgroundColor: AppTheme.current.surface,
        duration: Duration(seconds: 2),
      ),
    );
  }

  void _startGroupCall() {
    final memberIds = _members
        .map((m) => m['peer_id']?.toString() ?? '')
        .where((id) => id.isNotEmpty && id != _client.localPeerId)
        .toList();

    if (memberIds.length > 8) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text("Group calls support up to 8 participants")),
      );
      return;
    }

    if (memberIds.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text("No other members to call")),
      );
      return;
    }

    // Show call type selection dialog
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("START GROUP CALL", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              "Call ${memberIds.length} member${memberIds.length > 1 ? 's' : ''} in ${widget.groupName}",
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
                    _initiateCall(0); // 0 = audio only
                  },
                ),
                _buildCallTypeOption(
                  icon: Icons.videocam_rounded,
                  label: "Video",
                  color: AppTheme.current.accent,
                  onTap: () {
                    Navigator.pop(context);
                    _initiateCall(2); // 2 = video + audio
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
    final memberIds = _members
        .map((m) => m['peer_id']?.toString() ?? '')
        .where((id) => id.isNotEmpty && id != _client.localPeerId)
        .toList();

    final callService = GroupCallService.instance;
    await callService.initialize();
    await callService.initiateGroupCall(widget.groupId, memberIds, mediaType);

    if (mounted) {
      Navigator.push(
        context,
        MaterialPageRoute(
          builder: (context) => GroupCallScreen(
            groupId: widget.groupId,
            groupName: widget.groupName,
            participantIds: memberIds,
          ),
        ),
      );
    }
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
    return result;
  }

  void _showInfo() async {
    final result = await showDialog<bool>(
      context: context,
      builder: (context) => _GroupInfoDialog(
        groupId: widget.groupId,
        groupName: widget.groupName,
        onUpdate: _loadMessages,
        contactNames: _contactNames,
        contactAvatars: _contactAvatars,
        isAdmin: _isAdmin,
      ),
    );
    if (result == true && mounted) Navigator.pop(context);
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
                        _showCreatePollDialog();
                      },
                    ),
                    _buildAttachmentItem(
                      icon: Icons.checklist_rounded,
                      color: Colors.tealAccent,
                      label: "LIST",
                      onTap: () {
                        Navigator.pop(context);
                        _showActiveListSheet();
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

  void _pickAndSendImage() async {
    try {
      final pickedFiles = await ImagePicker().pickMultiImage(imageQuality: 100);
      if (pickedFiles.isNotEmpty) {
        for (var pickedFile in pickedFiles) {
          String path = pickedFile.path;
          String filename = pickedFile.name;
          final ext = path.split('.').last.toLowerCase();
          // Convert HEIC/HEIF to JPEG for universal compatibility
          if (ext == 'heic' || ext == 'heif') {
            path = await _convertHeicToJpeg(path);
            filename = filename.replaceAll(RegExp(r'\.(heic|heif)$', caseSensitive: false), '.jpg');
          }
          final file = File(path);
          final size = await file.length();
          final fileHash = _client.computeFileHash(path);
          final transferId = "gft_${fileHash}_${DateTime.now().millisecondsSinceEpoch}";
          _client.registerSeeder(transferId, path, fileHash, size, widget.groupId);
          final manifest = "[FILE]:${json.encode({
            "transfer_id": transferId,
            "sender_peer_id": _client.localPeerId,
            "filename": filename,
            "mime_type": "image/jpeg",
            "total_size": size,
            "file_hash": fileHash,
            "is_relayed": true,
            "group_id": widget.groupId,
          })}";
          _client.sendGroupMessage(widget.groupId, manifest);
        }
        _loadMessages();
      }
    } catch (_) {}
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
    return heicPath;
  }

  void _pickAndSendVideo() async {
    try {
      final pickedFile = await ImagePicker().pickVideo(source: ImageSource.gallery);
      if (pickedFile != null) {
        final file = File(pickedFile.path);
        final size = await file.length();
        final fileHash = _client.computeFileHash(pickedFile.path);
        final transferId = "gft_${fileHash}_${DateTime.now().millisecondsSinceEpoch}";
        _client.registerSeeder(transferId, pickedFile.path, fileHash, size, widget.groupId);
        final manifest = "[FILE]:${json.encode({
          "transfer_id": transferId,
          "sender_peer_id": _client.localPeerId,
          "filename": pickedFile.name,
          "mime_type": "video/mp4",
          "total_size": size,
          "file_hash": fileHash,
          "is_relayed": true,
          "group_id": widget.groupId,
        })}";
        _client.sendGroupMessage(widget.groupId, manifest);
        _loadMessages();
      }
    } catch (_) {}
  }

  void _sendFile() async {
    try {
      final result = await FilePicker.platform.pickFiles(type: FileType.any);
      if (result != null && result.files.single.path != null) {
        final path = result.files.single.path!;
        final file = File(path);
        final size = await file.length();
        final fileHash = _client.computeFileHash(path);
        final transferId = "gft_${fileHash}_${DateTime.now().millisecondsSinceEpoch}";
        _client.registerSeeder(transferId, path, fileHash, size, widget.groupId);
        final manifest = "[FILE]:${json.encode({
          "transfer_id": transferId,
          "sender_peer_id": _client.localPeerId,
          "filename": result.files.single.name,
          "mime_type": "application/octet-stream",
          "total_size": size,
          "file_hash": fileHash,
          "is_relayed": true,
          "group_id": widget.groupId,
        })}";
        _client.sendGroupMessage(widget.groupId, manifest);
        _loadMessages();
      }
    } catch (_) {}
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
        _client.sendGroupMessage(widget.groupId, text);
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
  void _showActiveListSheet() {
    final titleController = TextEditingController(text: _activeListTitle);
    List<Map<String, dynamic>> items = List.from(_activeListItems.map((i) => Map<String, dynamic>.from(i)));
    if (items.isEmpty) {
      items = [
        {"text": "", "checked": false},
        {"text": "", "checked": false},
      ];
    }

    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setSheetState) => Padding(
          padding: EdgeInsets.only(bottom: MediaQuery.of(ctx).viewInsets.bottom),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const SizedBox(height: 12),
              Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
              Padding(
                padding: const EdgeInsets.all(20),
                child: Text("ACTIVE LIST", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: TextField(
                  controller: titleController,
                  style: TextStyle(color: AppTheme.current.text),
                  decoration: InputDecoration(
                    hintText: "List title...",
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                    filled: true,
                    fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                    border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Flexible(
                child: ListView.builder(
                  shrinkWrap: true,
                  padding: const EdgeInsets.symmetric(horizontal: 20),
                  itemCount: items.length,
                  itemBuilder: (context, i) {
                    return Row(
                      children: [
                        Checkbox(
                          value: items[i]['checked'] ?? false,
                          onChanged: (val) {
                            setSheetState(() => items[i]['checked'] = val ?? false);
                          },
                          activeColor: AppTheme.current.accent,
                        ),
                        Expanded(
                          child: TextField(
                            controller: TextEditingController(text: items[i]['text'] ?? ''),
                            onChanged: (val) => items[i]['text'] = val,
                            style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                            decoration: InputDecoration(
                              hintText: "Item ${i + 1}",
                              hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.4)),
                              isDense: true,
                              contentPadding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
                              border: InputBorder.none,
                            ),
                          ),
                        ),
                        if (items.length > 2)
                          IconButton(
                            onPressed: () => setSheetState(() => items.removeAt(i)),
                            icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                          ),
                      ],
                    );
                  },
                ),
              ),
              TextButton.icon(
                onPressed: () {
                  setSheetState(() => items.add({"text": "", "checked": false}));
                },
                icon: Icon(Icons.add, size: 18, color: AppTheme.current.accent),
                label: Text("Add item", style: TextStyle(color: AppTheme.current.accent, fontSize: 12)),
              ),
              const SizedBox(height: 8),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    onPressed: () {
                      final title = titleController.text.trim();
                      if (title.isEmpty) {
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(content: Text("Enter a list title")),
                        );
                        return;
                      }
                      _saveAndSendActiveList(title, items);
                      Navigator.pop(ctx);
                    },
                    style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
                    child: const Text("SAVE & SHARE"),
                  ),
                ),
              ),
              const SizedBox(height: 20),
            ],
          ),
        ),
      ),
    );
  }
  void _showCreatePollDialog() {
    final questionController = TextEditingController();
    final List<TextEditingController> optionControllers = [
      TextEditingController(),
      TextEditingController(),
    ];

    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setSheetState) => Padding(
          padding: EdgeInsets.only(bottom: MediaQuery.of(ctx).viewInsets.bottom),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const SizedBox(height: 12),
              Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
              Padding(
                padding: const EdgeInsets.all(20),
                child: Text("CREATE POLL", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: TextField(
                  controller: questionController,
                  style: TextStyle(color: AppTheme.current.text),
                  decoration: InputDecoration(
                    hintText: "Ask a question...",
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                    filled: true,
                    fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                    border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
                  ),
                ),
              ),
              const SizedBox(height: 16),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: Text("OPTIONS", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1)),
              ),
              ...List.generate(optionControllers.length, (i) => Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 4),
                child: Row(
                  children: [
                    Container(
                      width: 24, height: 24,
                      decoration: BoxDecoration(shape: BoxShape.circle, color: AppTheme.current.accent.withValues(alpha: 0.2)),
                      child: Center(child: Text("${i + 1}", style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold))),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: TextField(
                        controller: optionControllers[i],
                        style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                        decoration: InputDecoration(
                          hintText: "Option ${i + 1}",
                          hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.4)),
                          isDense: true,
                          contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                          filled: true,
                          fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                          border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
                        ),
                      ),
                    ),
                    if (optionControllers.length > 2)
                      IconButton(
                        onPressed: () {
                          setSheetState(() => optionControllers.removeAt(i));
                        },
                        icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                      ),
                  ],
                ),
              )),
              if (optionControllers.length < 6)
                TextButton.icon(
                  onPressed: () {
                    setSheetState(() => optionControllers.add(TextEditingController()));
                  },
                  icon: Icon(Icons.add, size: 18, color: AppTheme.current.accent),
                  label: Text("Add option", style: TextStyle(color: AppTheme.current.accent, fontSize: 12)),
                ),
              const SizedBox(height: 8),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    onPressed: () {
                      final question = questionController.text.trim();
                      final options = optionControllers
                          .map((c) => c.text.trim())
                          .where((t) => t.isNotEmpty)
                          .toList();
                      if (question.isEmpty || options.length < 2) {
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(content: Text("Need a question and at least 2 options")),
                        );
                        return;
                      }
                      _sendPoll(question, options);
                      Navigator.pop(ctx);
                    },
                    style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
                    child: const Text("CREATE POLL"),
                  ),
                ),
              ),
              const SizedBox(height: 20),
            ],
          ),
        ),
      ),
    );
  }

  void _sendPoll(String question, List<String> options) {
    final pollId = "poll_${DateTime.now().millisecondsSinceEpoch}";
    final payload = "[POLL_CREATE]:${json.encode({
      "poll_id": pollId,
      "question": question,
      "options": options,
    })}";
    final localMsg = [_client.localPeerId, "me", payload, DateTime.now().toUtc().toIso8601String() + "Z", const Uuid().v4(), null];
    setState(() {
      _messages.add(localMsg);
      _polls[pollId] = {for (var opt in options) opt: <String>[]};
      _pollQuestions[pollId] = question;
      _pollOptions[pollId] = options;
    });
    _scrollToBottom();
    _client.sendGroupMessage(widget.groupId, payload);
  }

  void _votePoll(String pollId, int optionIndex) {
    final opts = _pollOptions[pollId];
    if (opts == null || optionIndex >= opts.length) return;
    final optionName = opts[optionIndex];

    final payload = "[POLL_VOTE]:${json.encode({
      "poll_id": pollId,
      "option_index": optionIndex,
      "voter": _client.localPeerId,
    })}";

    setState(() {
      _polls.putIfAbsent(pollId, () => {for (var opt in opts) opt: <String>[]});
      for (var opt in opts) {
        _polls[pollId]![opt]!.remove(_client.localPeerId);
      }
      _polls[pollId]![optionName]!.add(_client.localPeerId!);
    });

    _client.sendGroupMessage(widget.groupId, payload);
  }
  Widget _buildRightActionButton() {
    if (!_isInputEmpty || _editingMsgId != null) {
      return IconButton(
        onPressed: _sendMessage,
        icon: Icon(_editingMsgId != null ? Icons.check_circle_outline : Icons.send_rounded, color: AppTheme.current.accent),
      );
    }
    return Icon(Icons.mic_none, color: AppTheme.current.accent);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        backgroundColor: AppTheme.current.surface,
        title: InkWell(
          onTap: _showInfo,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(widget.groupName, style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: AppTheme.current.text)),
              Text("${_members.length} members", style: TextStyle(fontSize: 11, color: AppTheme.current.mutedText.withValues(alpha: 0.8), fontWeight: FontWeight.w400))
            ],
          ),
        ),
        actions: [
          if (_members.length <= 8)
            IconButton(
              onPressed: _startGroupCall,
              icon: Icon(Icons.videocam_rounded, color: AppTheme.current.accent),
              tooltip: 'Start Group Call',
            ),
          IconButton(
            onPressed: _showInfo,
            icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText.withValues(alpha: 0.7)),
          )
        ],
      ),
      body: Stack(
        children: [
          const SovereignWallpaper(),
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
              // Ongoing call banner (only show if <= 8 participants)
              if (_activeCallId != null && _activeCallMembers.length <= 8) _buildOngoingCallBanner(),
              Expanded(
                child: ListView.builder(
                  controller: _scrollController,
                  padding: const EdgeInsets.symmetric(vertical: 20),
                  itemCount: _displayMessages.length,
                  itemBuilder: (context, index) {
                    final msg = _displayMessages[index];
                    bool isMe = false;
                    String senderId = "";
                    String? replyTo;
                    DateTime msgDate = DateTime.now();

                    if (msg is FileTransferProgress) { 
                      isMe = msg.peerId == _client.localPeerId; 
                      senderId = msg.peerId; 
                      msgDate = DateTime.fromMillisecondsSinceEpoch(msg.startTimeMs);
                    } else if (msg is ImageGroupProgress) {
                      isMe = msg.images.first.peerId == _client.localPeerId; 
                      senderId = msg.images.first.peerId; 
                      msgDate = DateTime.fromMillisecondsSinceEpoch(msg.startTimeMs);
                    } else { 
                      senderId = msg[0]?.toString() ?? ''; 
                      isMe = senderId == _client.localPeerId; 
                      replyTo = msg.length > 5 ? msg[5]?.toString() : null; 
                      msgDate = _parseTimestamp(msg[3].toString());
                    }

                    bool showDateSeparator = false;
                    if (index == 0) {
                      showDateSeparator = true;
                    } else {
                      final prevMsg = _displayMessages[index - 1];
                      DateTime prevDate = DateTime.now();
                      if (prevMsg is FileTransferProgress) {
                        prevDate = DateTime.fromMillisecondsSinceEpoch(prevMsg.startTimeMs);
                      } else if (prevMsg is ImageGroupProgress) {
                        prevDate = DateTime.fromMillisecondsSinceEpoch(prevMsg.startTimeMs);
                      } else {
                        prevDate = _parseTimestamp(prevMsg[3]?.toString());
                      }
                      
                      if (msgDate.year != prevDate.year || msgDate.month != prevDate.month || msgDate.day != prevDate.day) {
                        showDateSeparator = true;
                      }
                    }

                    String displayName = _contactNames[senderId] ?? senderId;
                    if (displayName.length > 30) {
                      displayName = "Peer: ${displayName.substring(0, 6)}...${displayName.substring(displayName.length - 4)}";
                    }

                    final avatarWidget = SovereignAvatar(
                      radius: 21,
                      avatar: isMe 
                          ? (_myAvatar != null ? MemoryImage(base64Decode(_myAvatar!)) : null) 
                          : (_contactAvatars[senderId] != null ? MemoryImage(base64Decode(_contactAvatars[senderId]!)) : null),
                    );
                    
                    final bubble = Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
                      child: GestureDetector(
                        onHorizontalDragUpdate: (details) { if (details.delta.dx > 8) setState(() => _replyingTo = msg); },
                        onLongPress: () => _showMessageActions(msg),
                        child: Row(
                          mainAxisAlignment: isMe ? MainAxisAlignment.end : MainAxisAlignment.start,
                          crossAxisAlignment: CrossAxisAlignment.end,
                          children: [
                            if (!isMe) ...[avatarWidget, const SizedBox(width: 8)],
                            Flexible(
                              child: Column(
                                crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
                                children: [
                                  if (!isMe) Text(displayName, style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold)),
                                  _buildBubbleContent(msg, replyTo),
                                ],
                              ),
                            ),
                            if (isMe) ...[const SizedBox(width: 8), avatarWidget],
                          ],
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
              _buildInput(),
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
        ],
      ),
    );
  }

  Widget _buildInput() {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (_replyingTo != null) Container(padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)), left: BorderSide(color: AppTheme.current.accent, width: 4))), child: Row(children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text("Replying to ${_replyingTo is FileTransferProgress ? 'File' : (_replyingTo[0] == _client.localPeerId ? 'me' : (_contactNames[_replyingTo[0]] ?? 'mesh peer'))}", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold)), Text(_replyingTo is FileTransferProgress ? _replyingTo.filename : _replyingTo[2].toString(), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12))])), IconButton(icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.7)), onPressed: () => setState(() => _replyingTo = null))])),
        if (_editingMsgId != null) Container(padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)), left: BorderSide(color: Colors.orangeAccent, width: 4))), child: Row(children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text("EDITING MESSAGE", style: TextStyle(color: Colors.orangeAccent, fontSize: 10, fontWeight: FontWeight.bold)), Text(_messageController.text, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12))])), IconButton(icon: Icon(Icons.close, size: 18, color: AppTheme.current.mutedText.withValues(alpha: 0.7)), onPressed: () { setState(() { _editingMsgId = null; _messageController.clear(); }); })])),
        Container(padding: EdgeInsets.symmetric(horizontal: 16, vertical: 12), decoration: BoxDecoration(color: AppTheme.current.surface, border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)))), child: SafeArea(child: Row(children: [IconButton(onPressed: _showAttachmentOptions, icon: Icon(Icons.attach_file_rounded, color: AppTheme.current.accent)), IconButton(onPressed: () => setState(() => _showPanel = !_showPanel), icon: Icon(_showPanel ? Icons.keyboard_rounded : Icons.sentiment_satisfied_alt_rounded, color: AppTheme.current.accent)), SizedBox(width: 8), Expanded(child: _isRecording ? SizedBox.shrink() : TextField(controller: _messageController, style: TextStyle(color: AppTheme.current.text, fontSize: 15), decoration: InputDecoration(hintText: _replyingTo != null ? "WRITE YOUR REPLY..." : (_editingMsgId != null ? "EDIT MESSAGE..." : "Broadcast to mesh..."), hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)), border: OutlineInputBorder(borderRadius: BorderRadius.circular(24), borderSide: BorderSide.none), filled: true, fillColor: AppTheme.current.text.withValues(alpha: 0.05), contentPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 10)), onSubmitted: (_) => _sendMessage())), SizedBox(width: 12), _buildRightActionButton()]))),
      ],
    );
  }

  Widget _buildOngoingCallBanner() {
    final callerName = _contactNames[_activeCallCallerId] ?? _activeCallCallerId?.substring(0, 8) ?? 'Someone';
    final participantCount = _activeCallMembers.length + 1; // +1 for caller
    
    return GestureDetector(
      onTap: _joinActiveCall,
      child: Container(
        margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        decoration: BoxDecoration(
          gradient: LinearGradient(
            colors: [
              AppTheme.current.accent.withValues(alpha: 0.3),
              AppTheme.current.accent.withValues(alpha: 0.1),
            ],
          ),
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.5), width: 1.5),
          boxShadow: [
            BoxShadow(
              color: AppTheme.current.accent.withValues(alpha: 0.2),
              blurRadius: 8,
              spreadRadius: 1,
            ),
          ],
        ),
        child: Row(
          children: [
            // Pulsing icon
            AnimatedBuilder(
              animation: AlwaysStoppedAnimation(1.0),
              builder: (context, child) => Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: AppTheme.current.accent.withValues(alpha: 0.3),
                ),
                child: Icon(
                  _activeCallMediaType == 0 ? Icons.phone_rounded : Icons.videocam_rounded,
                  color: AppTheme.current.accent,
                  size: 20,
                ),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    "Group ${_activeCallMediaType == 0 ? 'Audio' : 'Video'} Call",
                    style: TextStyle(
                      color: AppTheme.current.accent,
                      fontSize: 12,
                      fontWeight: FontWeight.bold,
                      letterSpacing: 0.5,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    "$callerName + ${participantCount - 1} other${participantCount > 2 ? 's' : ''}",
                    style: TextStyle(
                      color: AppTheme.current.text.withValues(alpha: 0.7),
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
            // Join button
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              decoration: BoxDecoration(
                color: AppTheme.current.accent.withValues(alpha: 0.2),
                borderRadius: BorderRadius.circular(16),
                border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.5)),
              ),
              child: Text(
                "JOIN",
                style: TextStyle(
                  color: AppTheme.current.accent,
                  fontSize: 11,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 1,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _joinActiveCall() {
    if (_activeCallId == null || _activeCallCallerId == null) return;

    if (_activeCallMembers.length > 8) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text("Group calls support up to 8 participants")),
      );
      return;
    }
    
    final callService = GroupCallService.instance;
    callService.initialize().then((_) {
      if (!mounted) return;
      callService.joinGroupCall(
        _activeCallId!,
        widget.groupId,
        _activeCallMembers,
        _activeCallMediaType,
      );
      
      Navigator.push(
        context,
        MaterialPageRoute(
          builder: (context) => GroupCallScreen(
            groupId: widget.groupId,
            groupName: widget.groupName,
            participantIds: _activeCallMembers,
          ),
        ),
      );
      
      setState(() {
        _activeCallId = null;
        _activeCallCallerId = null;
        _activeCallMembers = [];
      });
    });
  }
}

class _GroupInfoDialog extends StatefulWidget {
  final String groupId;
  final String groupName;
  final VoidCallback onUpdate;
  final Map<String, String> contactNames;
  final Map<String, String> contactAvatars;
  final bool isAdmin;
  const _GroupInfoDialog({required this.groupId, required this.groupName, required this.onUpdate, required this.contactNames, required this.contactAvatars, required this.isAdmin});
  @override
  State<_GroupInfoDialog> createState() => _GroupInfoDialogState();
}

class _GroupInfoDialogState extends State<_GroupInfoDialog> {
  final IntrovertClient _client = IntrovertClient();
  List<dynamic> _members = [];
  List<String> _mutedMembers = [];
  String _description = "";
  int _retentionSeconds = 0;

  @override
  void initState() {
    super.initState();
    _loadMembers();
    _loadRetention();
    _loadMuted();
  }

  void _loadMuted() {
    setState(() {
      _mutedMembers = _client.getGroupMutedMembers(widget.groupId);
    });
  }

  void _loadMembers() {
    final groups = _client.getAllGroups();
    final currentGroup = groups.firstWhere((g) => g[0] == widget.groupId, orElse: () => null);
    if (currentGroup != null) {
      setState(() {
        _members = json.decode(currentGroup[2] as String);
        _description = currentGroup[3] as String;
      });
    }
  }
  
  void _refreshProfiles() {
    for (var m in _members) {
      final pid = m['peer_id']?.toString() ?? '';
      if (pid.isNotEmpty && pid != _client.localPeerId) {
        _client.pollPeerProfile(pid);
      }
    }
    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Refreshing mesh profiles...")));
  }

  void _loadRetention() {
    final groups = _client.getAllGroups();
    final currentGroup = groups.firstWhere((g) => g[0] == widget.groupId, orElse: () => null);
    if (currentGroup != null && currentGroup.length > 4) {
       _retentionSeconds = currentGroup[4] as int? ?? 0;
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
    return ListTile(
      title: Text(label, style: TextStyle(color: AppTheme.current.text)),
      trailing: _retentionSeconds == seconds ? Icon(Icons.check, color: AppTheme.current.accent) : null,
      onTap: () {
        _client.setRetention(widget.groupId, seconds, true);
        setState(() => _retentionSeconds = seconds);
        Navigator.pop(context);
      },
    );
  }

  void _showMemberOptions(String peerId, String name, bool isMuted) {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          Padding(
            padding: const EdgeInsets.all(16),
            child: Text(name.toUpperCase(), style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
          ),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: Icon(isMuted ? Icons.mic : Icons.mic_off, color: AppTheme.current.accent),
              title: Text(isMuted ? "Unmute Member" : "Mute Member", style: TextStyle(color: AppTheme.current.text)),
              onTap: () {
                if (isMuted) {
                  _client.unmuteMember(widget.groupId, peerId);
                } else {
                  _client.muteMember(widget.groupId, peerId);
                }
                Navigator.pop(ctx);
                _loadMuted();
              },
            ),
          ),
          Material(
            color: Colors.transparent,
            child: ListTile(
              leading: const Icon(Icons.person_remove, color: Colors.redAccent),
              title: const Text("Remove from Mesh", style: TextStyle(color: Colors.redAccent)),
              onTap: () async {
                Navigator.pop(ctx);
                final confirm = await showDialog<bool>(
                  context: context,
                  builder: (ctx) => AlertDialog(
                    backgroundColor: AppTheme.current.surface,
                    title: const Text("Remove Member?"),
                    content: Text("Are you sure you want to remove $name from the mesh?"),
                    actions: [
                      TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text("CANCEL")),
                      TextButton(onPressed: () => Navigator.pop(ctx, true), child: const Text("REMOVE", style: TextStyle(color: Colors.redAccent))),
                    ],
                  ),
                );
                if (confirm == true) {
                  _client.removeGroupMember(widget.groupId, peerId);
                  _loadMembers();
                  widget.onUpdate();
                }
              },
            ),
          ),
          const SizedBox(height: 20),
        ],
      ),
    );
  }

  void _addMember() async {
    final contacts = _client.getContacts();
    final List<String> currentMemberIds = _members.map((m) => m['peer_id'].toString()).toList();
    final List<dynamic> available = contacts.where((c) => !currentMemberIds.contains(c['peer_id'])).toList();
    if (available.isEmpty) { ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("No more contacts to add."))); return; }
    final String? selected = await showDialog<String>(context: context, builder: (context) => AlertDialog(backgroundColor: AppTheme.current.surface, title: Text("ADD MEMBER"), content: SizedBox(width: double.maxFinite, child: ListView.builder(shrinkWrap: true, itemCount: available.length, itemBuilder: (context, index) { final c = available[index]; return Material(color: Colors.transparent, child: ListTile(leading: SovereignAvatar(radius: 24, avatar: c['avatar'] != null ? MemoryImage(base64Decode(c['avatar'])) : null), title: Text(c['alias'] ?? c['peer_id'], style: TextStyle(color: AppTheme.current.text)), onTap: () => Navigator.pop(context, c['peer_id']))); }))));
    if (selected != null) { _client.addGroupMember(widget.groupId, selected); _loadMembers(); widget.onUpdate(); }
  }

  void _generateInviteCode() {
    // Generate a random 6 character alphanumeric code
    final code = const Uuid().v4().substring(0, 6).toUpperCase();
    _client.publishGroupManifest(widget.groupId, code);
    
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("GROUP INVITE CODE", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text("Share this code with non-contacts. They can use it to request access to this sovereign mesh room.", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
            const SizedBox(height: 24),
            Container(
              padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 32),
              decoration: BoxDecoration(
                color: AppTheme.current.accent.withValues(alpha: 0.1),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3)),
              ),
              child: Text(code, style: TextStyle(color: AppTheme.current.accent, fontSize: 24, fontWeight: FontWeight.bold, letterSpacing: 4)),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () {
              Clipboard.setData(ClipboardData(text: code));
              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Code copied to clipboard")));
              Navigator.pop(context);
            },
            child: Text("COPY", style: TextStyle(color: AppTheme.current.accent)),
          ),
          TextButton(onPressed: () => Navigator.pop(context), child: Text("CLOSE", style: TextStyle(color: AppTheme.current.mutedText))),
        ],
      ),
    );
  }

  void _leaveGroup() async {
    final confirm = await showDialog<bool>(context: context, builder: (context) => AlertDialog(backgroundColor: AppTheme.current.surface, title: Text("Leave Group?"), content: Text("You will no longer receive mesh updates for this room."), actions: [TextButton(onPressed: () => Navigator.pop(context, false), child: Text("CANCEL")), TextButton(onPressed: () => Navigator.pop(context, true), child: Text("LEAVE", style: TextStyle(color: Colors.redAccent)))]));
    if (confirm == true && mounted) { _client.removeGroupMember(widget.groupId, _client.localPeerId ?? ""); Navigator.pop(context, true); }
  }
  String _formatRetention(int seconds) {
    if (seconds == 0) return "Off";
    if (seconds < 3600) return "${seconds ~/ 60} minutes";
    if (seconds < 86400) return "${seconds ~/ 3600} hours";
    return "${seconds ~/ 86400} days";
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.current.surface, 
      title: Text(widget.groupName.toUpperCase(), style: TextStyle(color: AppTheme.current.accent, fontFamily: 'monospace', letterSpacing: 1.5)), 
      content: SizedBox(width: double.maxFinite, child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        if (_description.isNotEmpty) ...[Text(_description, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)), SizedBox(height: 16)], 
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: Icon(Icons.timer_outlined, color: AppTheme.current.accent),
          title: Text("Disappearing Messages", style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
          subtitle: Text(_formatRetention(_retentionSeconds), style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
          trailing: widget.isAdmin ? Icon(Icons.chevron_right, size: 18) : null,
          onTap: widget.isAdmin ? _showRetentionPicker : null,
        ),
        Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
        Text("MESH PARTICIPANTS", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 10, fontWeight: FontWeight.bold)), 
        SizedBox(height: 8), 
        Flexible(child: ListView.builder(shrinkWrap: true, itemCount: _members.length, itemBuilder: (context, index) { 
          final m = _members[index]; 
          final pid = m['peer_id'].toString(); 
          final role = m['role'].toString(); 
          final name = pid == _client.localPeerId ? "me" : (widget.contactNames[pid] ?? pid); 
          final avatarData = widget.contactAvatars[pid] ?? m['avatar']?.toString();
          final isMuted = _mutedMembers.contains(pid);

          return ListTile(
            contentPadding: EdgeInsets.zero, 
            leading: SovereignAvatar(
              radius: 21, 
              avatar: avatarData != null ? MemoryImage(base64Decode(avatarData)) : null,
              initials: name.isNotEmpty ? name[0].toUpperCase() : "?"
            ), 
            title: Row(
              children: [
                Expanded(child: Text(name, style: TextStyle(color: AppTheme.current.text, fontSize: 13), overflow: TextOverflow.ellipsis)),
                if (isMuted) ...[
                  const SizedBox(width: 4),
                  Icon(Icons.mic_off, size: 10, color: Colors.redAccent.withValues(alpha: 0.7)),
                ],
              ],
            ), 
            trailing: Text(role.toUpperCase(), style: TextStyle(color: role == "Creator" ? Colors.orangeAccent : AppTheme.current.accent, fontSize: 8, fontWeight: FontWeight.bold)),
            onTap: (widget.isAdmin && pid != _client.localPeerId) ? () => _showMemberOptions(pid, name, isMuted) : null,
          ); 
        })), 
        SizedBox(height: 16),
        TextButton.icon(
          onPressed: _refreshProfiles, 
          icon: Icon(Icons.refresh, size: 18, color: AppTheme.current.accent), 
          label: Text("REFRESH PROFILES", style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold))
        ),
        if (widget.isAdmin) ...[
          SizedBox(height: 8), 
          TextButton.icon(onPressed: _addMember, icon: Icon(Icons.person_add, size: 18), label: Text("ADD TO MESH")),
          SizedBox(height: 8), 
          TextButton.icon(onPressed: _generateInviteCode, icon: Icon(Icons.qr_code, size: 18), label: Text("SHARE INVITE CODE")),
        ],
        SizedBox(height: 8),
        TextButton.icon(
          onPressed: () {
            final memberIds = widget.contactNames.keys.toList();
            for (final memberId in memberIds) {
              _client.pollPeerProfile(memberId);
              _client.syncChatMessages(memberId, widget.groupId, true);
            }
            // Round 2 after relay propagation
            Future.delayed(Duration(seconds: 2), () {
              for (final memberId in memberIds) {
                _client.syncChatMessages(memberId, widget.groupId, true);
              }
            });
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: Text("Syncing ${memberIds.length} contacts & messages (2 rounds)...", style: TextStyle(color: AppTheme.current.accent)),
                backgroundColor: AppTheme.current.surface,
              ),
            );
          },
          icon: Icon(Icons.sync, size: 18, color: AppTheme.current.accent),
          label: Text("SYNC CONTACTS & MESSAGES", style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold))
        ),
        SizedBox(height: 8),
        TextButton.icon(
          onPressed: () {
            final memberIds = widget.contactNames.keys.toList();
            for (final memberId in memberIds) {
              _client.syncChatMessages(memberId, widget.groupId, true, isFull: true);
            }
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: Text("Syncing full group history...", style: TextStyle(color: AppTheme.current.accent)),
                backgroundColor: AppTheme.current.surface,
              ),
            );
          },
          icon: Icon(Icons.sync, size: 18, color: AppTheme.current.accent),
          label: Text("SYNC FULL HISTORY", style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold))
        ),
        SizedBox(height: 8),
        TextButton.icon(
          onPressed: () async {
            final confirm = await showDialog<bool>(context: context, builder: (ctx) => AlertDialog(backgroundColor: AppTheme.current.surface, title: const Text("Clear Chat?", style: TextStyle(color: Colors.redAccent)), content: Text("This will permanently delete all messages for this group from your device.", style: TextStyle(color: AppTheme.current.text)), actions: [TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text("CANCEL")), TextButton(onPressed: () => Navigator.pop(ctx, true), child: const Text("CLEAR", style: TextStyle(color: Colors.redAccent)))]));
            if (confirm == true && mounted) { _client.deleteChat(widget.groupId); widget.onUpdate(); Navigator.pop(context); }
          }, 
          icon: const Icon(Icons.delete_sweep, size: 18, color: Colors.orangeAccent), 
          label: const Text("CLEAR CHAT", style: TextStyle(color: Colors.orangeAccent, fontSize: 11, fontWeight: FontWeight.bold))
        )
      ])), 
      actions: [
        if (widget.isAdmin)
          TextButton(onPressed: () async {
            final confirm = await showDialog<bool>(context: context, builder: (ctx) => AlertDialog(backgroundColor: AppTheme.current.surface, title: const Text("Delete Group?", style: TextStyle(color: Colors.redAccent)), content: Text("This will permanently delete the group for all members.", style: TextStyle(color: AppTheme.current.text)), actions: [TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text("CANCEL")), TextButton(onPressed: () => Navigator.pop(ctx, true), child: const Text("DELETE", style: TextStyle(color: Colors.redAccent)))]));
            if (confirm == true && mounted) { _client.deleteGroup(widget.groupId); Navigator.pop(context, true); }
          }, child: Text("DELETE GROUP", style: TextStyle(color: Colors.redAccent)))
        else
          TextButton(onPressed: _leaveGroup, child: Text("LEAVE GROUP", style: TextStyle(color: Colors.redAccent))), 
        TextButton(onPressed: () => Navigator.pop(context), child: Text("CLOSE", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7))))
      ] 
    );
  }
}
