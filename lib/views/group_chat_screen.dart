import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import '../src/native/introvert_client.dart';
import '../src/ui/widgets/file_transfer_bubble.dart';

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
  StreamSubscription<NetworkEvent>? _networkSubscription;

  @override
  void initState() {
    super.initState();
    _loadMessages();
    _startListener();
    _transferSubscription = _client.transferStream.listen((progress) {
      if (mounted) {
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
    super.dispose();
  }

  void _loadMessages() {
    final msgs = _client.getGroupMessages(widget.groupId);
    setState(() {
      _messages = msgs.map((m) {
        final senderId = m[0] as String;
        final content = m[2] as String;
        
        if (content.startsWith("[FILE]:")) {
          try {
            final jsonStr = content.substring(7);
            final meta = json.decode(jsonStr);
            final tid = meta['transfer_id'] as String;
            
            if (_groupTransfers.containsKey(tid)) {
              return _groupTransfers[tid]!;
            }
            
            final localPath = meta['local_path'];
            final isOutgoing = senderId == _client.localPeerId;
            bool exists = false;
            if (localPath != null && localPath.toString().isNotEmpty) {
              exists = File(localPath.toString()).existsSync();
            }
            
            final progress = FileTransferProgress(
              transferId: tid,
              peerId: senderId,
              filename: meta['filename'] ?? 'unknown',
              mimeType: meta['mime_type'] ?? 'application/octet-stream',
              progress: (isOutgoing || exists) ? ((meta['progress'] as num?)?.toDouble() ?? 0.0) : 0.0,
              speedBps: (isOutgoing || exists) ? ((meta['speed_bps'] as num?)?.toDouble() ?? 0.0) : 0.0,
              isComplete: (isOutgoing || exists) ? (meta['is_complete'] ?? false) : false,
              isVerified: (isOutgoing || exists) ? (meta['is_verified'] ?? false) : false,
              isOutgoing: isOutgoing,
              isCancelled: meta['is_cancelled'] ?? false,
              localPath: (isOutgoing || exists) ? localPath : null,
              startTimeMs: meta['start_time_ms'] ?? 0,
            );
            return progress;
          } catch (e) {
            debugPrint("Error parsing group file message: $e");
          }
        }
        return m;
      }).toList();
    });
    _scrollToBottom();
  }

  void _startListener() {
    _networkSubscription = _client.networkStream.listen((event) {
      if (event.type == 21) {
        // Event 21: Group Message Received [GID_LEN, GID, SID_LEN, SID, Content]
        final data = event.data;
        final gidLen = data[0];
        final gid = utf8.decode(data.sublist(1, 1 + gidLen));
        
        if (gid == widget.groupId) {
          _loadMessages();
        }
      }
    });
  }

  void _sendFile() async {
    final result = await FilePicker.platform.pickFiles();
    if (result != null && result.files.single.path != null) {
      final path = result.files.single.path!;
      final filename = result.files.single.name;
      final file = File(path);
      final size = await file.length();
      
      final fileHash = "f_hash_${DateTime.now().millisecondsSinceEpoch}";
      final mimeType = result.files.single.extension ?? "bin";
      final transferId = "gft_${widget.groupId}_${DateTime.now().millisecondsSinceEpoch}";
      
      _client.registerSeeder(transferId, path, fileHash, size, widget.groupId);
      
      final fileShareMsg = "[FILE]:${json.encode({
        "transfer_id": transferId,
        "peer_id": _client.localPeerId,
        "filename": filename,
        "mime_type": mimeType,
        "progress": 1.0,
        "speed_bps": 0.0,
        "is_complete": true,
        "is_verified": true,
        "is_outgoing": true,
        "is_cancelled": false,
        "local_path": path,
        "file_hash": fileHash,
        "total_size": size,
        "start_time_ms": DateTime.now().millisecondsSinceEpoch,
      })}";
      
      _client.sendGroupMessage(widget.groupId, fileShareMsg);
      _loadMessages();
    }
  }

  void _sendMessage() {
    final text = _messageController.text.trim();
    if (text.isEmpty) return;
    
    // OPTIMISTIC UI: Add to local message list immediately
    final localMsg = [
      _client.localPeerId,
      "me",
      text,
      DateTime.now().toIso8601String(),
    ];

    setState(() {
      _messages.add(localMsg);
      _messageController.clear();
    });
    _scrollToBottom();

    // Send in background
    _client.sendGroupMessage(widget.groupId, text);
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
        );
      }
    });
  }

  void _showInfo() {
    showDialog(
      context: context,
      builder: (context) => _GroupInfoDialog(
        groupId: widget.groupId,
        groupName: widget.groupName,
        onUpdate: _loadMessages,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: const Color(0xFF0F1219),
      appBar: AppBar(
        backgroundColor: const Color(0xFF1A1F2B),
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(widget.groupName, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: Colors.white)),
            const Text("SOVEREIGN MESH ROOM", style: TextStyle(fontSize: 9, color: Colors.cyanAccent, letterSpacing: 1.2)),
          ],
        ),
        actions: [
          IconButton(onPressed: _showInfo, icon: const Icon(Icons.info_outline, color: Colors.white38)),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: ListView.builder(
              controller: _scrollController,
              padding: const EdgeInsets.all(16),
              itemCount: _messages.length,
              itemBuilder: (context, index) {
                final msg = _messages[index];
                if (msg is FileTransferProgress) {
                  final isMe = msg.peerId == _client.localPeerId;
                  Widget bubble = FileTransferBubble(
                    progress: msg,
                    isMe: isMe,
                  );
                  if (!isMe) {
                    bubble = Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Padding(
                          padding: const EdgeInsets.only(left: 36, bottom: 4),
                          child: Text(
                            "${msg.peerId.substring(0, 8)}...",
                            style: const TextStyle(color: Colors.white38, fontSize: 10, fontFamily: 'monospace'),
                          ),
                        ),
                        bubble,
                      ],
                    );
                  }
                  return GestureDetector(
                    onTap: () {
                      if (!msg.isComplete && !msg.isVerified && !isMe) {
                        final msgs = _client.getGroupMessages(widget.groupId);
                        final groupMsg = msgs.firstWhere(
                          (m) => m[2].startsWith("[FILE]:") && m[2].contains(msg.transferId),
                          orElse: () => null,
                        );
                        if (groupMsg != null) {
                          try {
                            final meta = json.decode(groupMsg[2].substring(7));
                            final fileHash = meta['file_hash'] as String;
                            final totalSize = meta['total_size'] as int;
                            _client.startPull(msg.peerId, msg.transferId, msg.filename, msg.mimeType, fileHash, totalSize, true, widget.groupId);
                            ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(content: Text("Requesting '${msg.filename}' from mesh...")),
                            );
                          } catch (e) {
                            debugPrint("Error parsing group message meta: $e");
                          }
                        }
                      }
                    },
                    child: bubble,
                  );
                }

                final senderId = msg[0] as String;
                final content = msg[2] as String;
                final timestamp = msg[3] as String;
                final isMe = senderId == _client.localPeerId;

                return _GroupMessageBubble(
                  senderId: senderId,
                  content: content,
                  timestamp: timestamp,
                  isMe: isMe,
                );
              },
            ),
          ),
          _buildInput(),
        ],
      ),
    );
  }

  Widget _buildInput() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: const BoxDecoration(
        color: Color(0xFF1A1F2B),
        border: Border(top: BorderSide(color: Colors.white10)),
      ),
      child: SafeArea(
        child: Row(
          children: [
            IconButton(
              onPressed: _sendFile,
              icon: const Icon(Icons.attach_file_rounded, color: Colors.cyanAccent),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                controller: _messageController,
                style: const TextStyle(color: Colors.white, fontSize: 15),
                decoration: InputDecoration(
                  hintText: "Broadcast to mesh...",
                  hintStyle: const TextStyle(color: Colors.white24),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(24),
                    borderSide: BorderSide.none,
                  ),
                  filled: true,
                  fillColor: Colors.white.withValues(alpha: 0.05),
                  contentPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 10),
                ),
                onSubmitted: (_) => _sendMessage(),
              ),
            ),
            const SizedBox(width: 12),
            IconButton(
              onPressed: _sendMessage,
              icon: const Icon(Icons.send_rounded, color: Colors.cyanAccent),
            ),
          ],
        ),
      ),
    );
  }
}

class _GroupMessageBubble extends StatelessWidget {
  final String senderId;
  final String content;
  final String timestamp;
  final bool isMe;

  const _GroupMessageBubble({
    required this.senderId,
    required this.content,
    required this.timestamp,
    required this.isMe,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: 16),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Column(
        crossAxisAlignment: isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
        children: [
          if (!isMe)
            Padding(
              padding: const EdgeInsets.only(left: 12, bottom: 4),
              child: Text(
                "${senderId.substring(0, 8)}...",
                style: const TextStyle(color: Colors.white38, fontSize: 10, fontFamily: 'monospace'),
              ),
            ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
            constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
            decoration: BoxDecoration(
              color: isMe ? Colors.cyanAccent.withValues(alpha: 0.15) : Colors.white.withValues(alpha: 0.05),
              borderRadius: BorderRadius.circular(20).copyWith(
                bottomRight: isMe ? const Radius.circular(0) : null,
                bottomLeft: !isMe ? const Radius.circular(0) : null,
              ),
              border: Border.all(color: isMe ? Colors.cyanAccent.withValues(alpha: 0.3) : Colors.white10),
            ),
            child: Text(
              content,
              style: const TextStyle(color: Colors.white, fontSize: 14),
            ),
          ),
        ],
      ),
    );
  }
}

class _GroupInfoDialog extends StatefulWidget {
  final String groupId;
  final String groupName;
  final VoidCallback onUpdate;

  const _GroupInfoDialog({
    required this.groupId,
    required this.groupName,
    required this.onUpdate,
  });

  @override
  State<_GroupInfoDialog> createState() => _GroupInfoDialogState();
}

class _GroupInfoDialogState extends State<_GroupInfoDialog> {
  final IntrovertClient _client = IntrovertClient();
  List<dynamic> _members = [];
  bool _isAdmin = false;
  bool _isCreator = false;
  String _description = "";

  @override
  void initState() {
    super.initState();
    _loadMembers();
  }

  void _loadMembers() {
    final groups = _client.getAllGroups();
    final current = groups.firstWhere((g) => g[0] == widget.groupId, orElse: () => null);
    if (current != null) {
      final List<dynamic> members = json.decode(current[2] as String);
      setState(() {
        _members = members;
        _description = current.length > 3 ? (current[3] as String) : "";
        final me = members.firstWhere((m) => m['peer_id'] == _client.localPeerId, orElse: () => null);
        _isAdmin = me != null && (me['role'] == "Creator" || me['role'] == "Admin");
        _isCreator = me != null && me['role'] == "Creator";
      });
    }
  }

  void _addMember() async {
    final contacts = _client.getContacts();
    final nonMembers = contacts.where((c) => !_members.any((m) => m['peer_id'] == c['peer_id'])).toList();

    if (nonMembers.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("All contacts are already in this mesh.")));
      return;
    }

    final selected = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xFF1A1F2B),
        title: const Text("Add to Mesh", style: TextStyle(color: Colors.white, fontSize: 16)),
        content: SizedBox(
          width: double.maxFinite,
          child: ListView.builder(
            shrinkWrap: true,
            itemCount: nonMembers.length,
            itemBuilder: (context, index) {
               final c = nonMembers[index];
               return ListTile(
                 title: Text(c['alias'] ?? c['peer_id'], style: const TextStyle(color: Colors.white70)),
                 onTap: () => Navigator.pop(context, c['peer_id']),
               );
            },
          ),
        ),
      ),
    );

    if (selected != null) {
      _client.addGroupMember(widget.groupId, selected);
      Future.delayed(const Duration(seconds: 1), _loadMembers);
    }
  }

  void _showMeshCode() async {
    final TextEditingController codeController = TextEditingController();
    final String? code = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xFF1A1F2B),
        title: const Text("Generate Mesh Code", style: TextStyle(color: Colors.white, fontSize: 16)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text("Create a human-readable passphrase (e.g., 'blue-sky-77') for others to join this room directly.", style: TextStyle(color: Colors.white38, fontSize: 12)),
            const SizedBox(height: 16),
            TextField(
              controller: codeController,
              autofocus: true,
              style: const TextStyle(color: Colors.white, fontFamily: 'monospace'),
              decoration: const InputDecoration(
                labelText: "NEW MESH CODE",
                labelStyle: TextStyle(color: Colors.cyanAccent),
                enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: Colors.white10)),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: const Text("CANCEL")),
          ElevatedButton(
            onPressed: () => Navigator.pop(context, codeController.text.trim()),
            style: ElevatedButton.styleFrom(backgroundColor: Colors.cyanAccent, foregroundColor: Colors.black),
            child: const Text("PUBLISH TO MESH"),
          ),
        ],
      ),
    );

    if (code != null && code.isNotEmpty) {
       _client.publishGroupManifest(widget.groupId, code);
       if (!mounted) return;
       ScaffoldMessenger.of(context).showSnackBar(SnackBar(
         backgroundColor: Colors.cyanAccent,
         content: Text("Mesh Code '$code' is now active!", style: const TextStyle(color: Colors.black, fontWeight: FontWeight.bold)),
       ));
    }
  }

  void _exitGroup() async {
    final confirm = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xFF1A1F2B),
        title: const Text("Exit Group?", style: TextStyle(color: Colors.redAccent)),
        content: const Text("Are you sure you want to exit this group? You will no longer receive messages from this group mesh."),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text("CANCEL")),
          TextButton(
            onPressed: () => Navigator.pop(context, true), 
            child: const Text("EXIT", style: TextStyle(color: Colors.redAccent)),
          ),
        ],
      ),
    );

    if (confirm == true) {
      _client.removeGroupMember(widget.groupId, _client.localPeerId ?? "");
      if (mounted) {
        Navigator.pop(context); // close info dialog
        Navigator.pop(context); // close chat screen
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text("You exited the group.")),
        );
        widget.onUpdate();
      }
    }
  }

  void _deleteGroup() async {
    final confirm = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xFF1A1F2B),
        title: const Text("Delete Group?", style: TextStyle(color: Colors.redAccent)),
        content: const Text("This will permanently delete this group room and its message history for all members. This cannot be undone."),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text("CANCEL")),
          TextButton(
            onPressed: () => Navigator.pop(context, true), 
            child: const Text("DELETE", style: TextStyle(color: Colors.redAccent)),
          ),
        ],
      ),
    );

    if (confirm == true) {
      _client.deleteGroup(widget.groupId);
      if (mounted) {
        Navigator.pop(context); // close info dialog
        Navigator.pop(context); // close chat screen
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text("Group deleted.")),
        );
        widget.onUpdate();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: const Color(0xFF1A1F2B),
      title: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          const Text("Mesh Intelligence", style: TextStyle(color: Colors.white, fontSize: 18)),
          if (_isAdmin)
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                IconButton(onPressed: _showMeshCode, icon: const Icon(Icons.qr_code_2, color: Colors.cyanAccent, size: 20)),
                IconButton(onPressed: _addMember, icon: const Icon(Icons.person_add, color: Colors.cyanAccent, size: 20)),
              ],
            ),
        ],
      ),
      content: SizedBox(
        width: double.maxFinite,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (_description.isNotEmpty) ...[
              const Text("DESCRIPTION", style: TextStyle(color: Colors.white38, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
              const SizedBox(height: 6),
              Text(
                _description,
                style: const TextStyle(color: Colors.white70, fontSize: 13),
              ),
              const SizedBox(height: 20),
            ],
            const Text("CURRENT MEMBERS", style: TextStyle(color: Colors.white38, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
            const SizedBox(height: 12),
            Flexible(
              child: ListView.builder(
                shrinkWrap: true,
                itemCount: _members.length,
                itemBuilder: (context, index) {
                  final member = _members[index];
                  final pid = member['peer_id'] as String;
                  final role = member['role'] as String;
                  final isMe = pid == _client.localPeerId;

                  return ListTile(
                    contentPadding: EdgeInsets.zero,
                    leading: CircleAvatar(
                      radius: 16,
                      backgroundColor: Colors.white10,
                      child: Text(role[0], style: const TextStyle(fontSize: 10, color: Colors.cyanAccent)),
                    ),
                    title: Text(
                      isMe ? "You ($role)" : "${pid.substring(0, 8)}... ($role)",
                      style: TextStyle(color: isMe ? Colors.cyanAccent : Colors.white70, fontSize: 13, fontFamily: 'monospace'),
                    ),
                    trailing: (_isAdmin && !isMe && role != "Creator") 
                      ? PopupMenuButton<String>(
                          icon: const Icon(Icons.more_vert, color: Colors.white24, size: 18),
                          onSelected: (val) {
                            if (val == "remove") {
                              _client.removeGroupMember(widget.groupId, pid);
                            } else if (val == "admin") {
                              _client.updateGroupRole(widget.groupId, pid, 1); // 1 = Admin
                            }
                            Future.delayed(const Duration(seconds: 1), _loadMembers);
                          },
                          itemBuilder: (context) => [
                            const PopupMenuItem(value: "admin", child: Text("Promote to Admin")),
                            const PopupMenuItem(value: "remove", child: Text("Remove from Mesh", style: TextStyle(color: Colors.redAccent))),
                          ],
                        )
                      : null,
                  );
                },
              ),
            ),
          ],
        ),
      ),
      actions: [
        if (_isCreator)
          TextButton(
            onPressed: _deleteGroup,
            child: const Text("DELETE GROUP", style: TextStyle(color: Colors.redAccent)),
          )
        else
          TextButton(
            onPressed: _exitGroup,
            child: const Text("EXIT GROUP", style: TextStyle(color: Colors.redAccent)),
          ),
        TextButton(onPressed: () => Navigator.pop(context), child: const Text("CLOSE")),
      ],
    );
  }
}
