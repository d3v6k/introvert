import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import '../src/native/introvert_client.dart';
import '../src/ui/widgets/rewards_hud.dart';
import '../src/ui/widgets/file_transfer_bubble.dart';
import '../src/ui/widgets/sovereign_avatar.dart';
import '../blueprint_ui.dart';
import 'call_screen.dart';

class ChatScreen extends StatefulWidget {
  final String peerId;
  final String? peerName;
  final String? avatarBase64;

  const ChatScreen({required this.peerId, this.peerName, this.avatarBase64, super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final List<dynamic> _messages = [];
  final IntrovertClient _client = IntrovertClient();
  
  bool _isE2eeActive = false;
  int _relayedBytes = 0;
  double _solRewards = 0.0;
  String _status = "Offline";
  Color _statusColor = Colors.redAccent;

  late final StreamSubscription<NetworkEvent> _networkSubscription;
  StreamSubscription<MediaFrameEvent>? _mediaSubscription;
  StreamSubscription<FileTransferProgress>? _transferSubscription;
  StreamSubscription? _economySubscription;

  @override
  void initState() {
    super.initState();
    _loadMessages();
    _startNetworkDiscovery();
    _startEconomyMonitor();
  }

  Future<void> _loadMessages() async {
    try {
      final messagesJson = _client.getMessages(widget.peerId);
      final List<dynamic> loaded = [];
      for (final m in messagesJson) {
        if (m is Map<String, dynamic>) {
          final content = m['content'] as String? ?? '';
          final timestampStr = m['timestamp'] as String? ?? '';
          final isMe = m['is_me'] == true || m['is_me'] == 1 || m['is_me'] == '1';
          int status = m['status'] as int? ?? 1;
          final msgId = m['msg_id'] as String?;
          
          DateTime ts;
          try {
            ts = DateTime.parse(timestampStr);
          } catch (_) {
            ts = DateTime.now();
          }
          
          if (content.startsWith("[FILE]:")) {
            try {
              final jsonStr = content.substring(7);
              final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
              loaded.add(progress);
              continue;
            } catch (e) {
              debugPrint("Failed to parse file transfer progress from db: $e");
            }
          }
          
          if (!isMe && status < 2 && msgId != null) {
            status = 2;
            _client.sendAcknowledgement(widget.peerId, msgId, 2);
          }
          
          loaded.add(MessageModel(
            content: content,
            isMe: isMe,
            timestamp: ts,
            status: status,
            msgId: msgId,
          ));
        }
      }
      setState(() {
        _messages.addAll(loaded);
      });
      _scrollToBottom();
    } catch (e) {
      debugPrint("Error loading messages: $e");
    }
  }

  void _startEconomyMonitor() {
    _economySubscription = _client.economyStream.listen((stats) {
      if (mounted) {
        setState(() {
          _solRewards = (stats['intr_balance'] ?? 0) / 1000000000.0;
        });
      }
    });
  }

  void _startNetworkDiscovery() {
    _client.startNetwork();
    _client.establishSecureSession(widget.peerId);
    _networkSubscription = _client.networkStream.listen((event) {
      if (event.type == 1) {
        // Kademlia/DHT Discovery Update
      } else if (event.type == 8) {
        // Event 8: [PeerIdString]:[StatusByte] (0=Direct, 1=Relay, 2=Offline)
        if (event.data.isEmpty) return;
        
        // Find the colon separator from the end
        int colonIndex = event.data.lastIndexOf(58); // 58 is ASCII for ':'
        if (colonIndex == -1 || colonIndex == event.data.length - 1) return;

        final eventPeerId = String.fromCharCodes(event.data.sublist(0, colonIndex));
        final statusCode = event.data[colonIndex + 1];
        
        // Filter connection events for this specific peer
        if (eventPeerId != widget.peerId && eventPeerId.isNotEmpty) return;

        setState(() {
          if (statusCode == 0) {
            _status = "Direct P2P";
            _statusColor = Colors.greenAccent;
          } else if (statusCode == 1) {
            _status = "Relay Active";
            _statusColor = Colors.orangeAccent;
          } else if (statusCode == 3) {
            _status = "Syncing...";
            _statusColor = Colors.cyanAccent;
          } else {
            _status = "Offline";
            _statusColor = Colors.redAccent;
            _isE2eeActive = false;
          }
        });
        
        // Auto-negotiate secure session if reachable
        if (statusCode == 0 || statusCode == 1) {
          _client.establishSecureSession(widget.peerId);
        }
      } else if (event.type == 2 || event.type == 4) {
        final data = event.data;
        if (data.length < 8) return;

        final tsBytes = Uint8List.fromList(data.sublist(0, 8));
        final timestamp = ByteData.sublistView(tsBytes).getInt64(0, Endian.big);

        String content;
        String? msgId;

        if (event.type == 2) {
          // Event 2: [8-byte Timestamp][1-byte msg_id_len][msg_id_bytes][content]
          if (data.length > 9) {
            final msgIdLen = data[8];
            if (data.length >= 9 + msgIdLen) {
              msgId = utf8.decode(data.sublist(9, 9 + msgIdLen));
              content = utf8.decode(data.sublist(9 + msgIdLen));
              if (msgId.isEmpty) msgId = null;
            } else {
              content = utf8.decode(data.sublist(8));
            }
          } else {
            content = utf8.decode(data.sublist(8));
          }
        } else {
          // Event 4 fallback: [8-byte Timestamp, Content...]
          content = utf8.decode(data.sublist(8));
        }

        if (content.startsWith("WEBRTC:")) return; // Ignore internal signaling
        
        setState(() {
          if (event.type == 2) _isE2eeActive = true; 
          
          if (content.startsWith("[FILE]:")) {
            try {
              final jsonStr = content.substring(7);
              final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
              final index = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
              if (index != -1) {
                _messages[index] = progress;
              } else {
                _messages.add(progress);
              }
              _scrollToBottom();
              return;
            } catch (e) {
              debugPrint("Failed to parse real-time file transfer: $e");
            }
          }

          // Since the ChatScreen is open, the incoming message is immediately visible/read.
          // Send Read receipt (status: 2) to peer.
          if (msgId != null) {
            _client.sendAcknowledgement(widget.peerId, msgId, 2);
          }

          _messages.add(MessageModel(
            content: content,
            isMe: false,
            timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
            status: msgId != null ? 2 : 1,
            msgId: msgId,
          ));
        });
        _scrollToBottom();
      } else if (event.type == 13) {
        // Event 13: [StatusByte, msg_id_bytes...]
        if (event.data.length < 2) return;
        final status = event.data[0];
        final msgId = utf8.decode(event.data.sublist(1));

        setState(() {
          final index = _messages.indexWhere((m) => m is MessageModel && m.msgId == msgId);
          if (index != -1) {
            (_messages[index] as MessageModel).status = status;
          }
        });
      }
    });

    _mediaSubscription = _client.mediaStream.listen((event) {
      // Chat screen doesn't render video directly now, navigate to CallScreen.
      // Free memory to prevent leaks.
      _client.freeBinary(event.basePtr, event.baseLen);
    });

    _transferSubscription = _client.transferStream.listen((progress) {
      if (progress.peerId != widget.peerId) return;
      
      setState(() {
        // Find existing transfer or add new one
        final index = _messages.indexWhere((m) => m is FileTransferProgress && m.transferId == progress.transferId);
        if (index != -1) {
          _messages[index] = progress;
        } else {
          _messages.add(progress);
        }
      });
      _scrollToBottom();
    });
  }

  @override
  void dispose() {
    _networkSubscription.cancel();
    _mediaSubscription?.cancel();
    _transferSubscription?.cancel();
    _economySubscription?.cancel();
    _messageController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _sendMessage() async {
    final text = _messageController.text.trim();
    if (text.isEmpty) return;

    try {
      final msgId = await _client.sendMessage(widget.peerId, text);

      setState(() {
        _messages.add(MessageModel(
          content: text,
          isMe: true,
          timestamp: DateTime.now(),
          status: 0, // Starts as 0 (Sent) -> Single Tick
          msgId: msgId,
        ));
        _messageController.clear();
        _relayedBytes += text.length;
        _solRewards = _relayedBytes * 0.0000001; // Mock incentive formula
      });
      _scrollToBottom();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Send failed: $e")),
        );
      }
    }
  }

  void _establishSecureSession() {
    try {
      _client.establishSecureSession(widget.peerId);
      setState(() => _isE2eeActive = true); // Optimistic UI
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text("Initiating E2EE Handshake...")),
      );
    } catch (e) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text("Handshake failed: $e")),
      );
    }
  }

  void _startVideoCall() {
    try {
      _client.initiateWebRtc(widget.peerId);
      _client.startMediaStream(widget.peerId, 2); // 2 = Audio + Video
      
      Navigator.push(
        context,
        MaterialPageRoute(builder: (context) => CallScreen(peerId: widget.peerId)),
      );
    } catch (e) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text("Call failed: $e")),
      );
    }
  }

  void _sendFile() async {
    final result = await FilePicker.platform.pickFiles();
    if (result != null && result.files.single.path != null) {
      _client.sendFile(widget.peerId, result.files.single.path!);
    }
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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: const Color(0xFF0A0E14),
      appBar: AppBar(
        backgroundColor: Colors.transparent,
        elevation: 0,
        leadingWidth: 100, // Make room for back button and avatar
        leading: Row(
          children: [
            IconButton(
              icon: const Icon(Icons.arrow_back_ios_new, size: 20, color: Colors.white),
              onPressed: () => Navigator.of(context).pop(),
            ),
            SovereignAvatar(
              radius: 18,
              avatar: widget.avatarBase64 != null ? MemoryImage(base64Decode(widget.avatarBase64!)) : null,
              balance: _solRewards, // _solRewards is already in INTR units from economyStream
              isSuperActive: _relayedBytes > 10 * 1024 * 1024,
            ),
          ],
        ),
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              widget.peerName ?? "PEER: ${widget.peerId}",
              style: TextStyle(
                fontSize: widget.peerName != null ? 16 : 10, 
                fontFamily: widget.peerName != null ? null : 'monospace', 
                fontWeight: FontWeight.bold,
                color: widget.peerName != null ? Colors.white : Colors.cyanAccent
              ),
              overflow: TextOverflow.ellipsis,
            ),
            Row(
              children: [
                Container(
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(shape: BoxShape.circle, color: _statusColor),
                ),
                const SizedBox(width: 4),
                Text(
                  _status.toUpperCase(),
                  style: TextStyle(fontSize: 8, color: _statusColor, letterSpacing: 1.1, fontWeight: FontWeight.bold),
                ),
              ],
            ),
          ],
        ),
        actions: [
          IconButton(
            icon: Icon(_isE2eeActive ? Icons.lock : Icons.lock_open, color: _isE2eeActive ? Colors.cyanAccent : Colors.white24, size: 20),
            tooltip: "E2EE Status",
            onPressed: _establishSecureSession,
          ),
          IconButton(
            icon: const Icon(Icons.videocam_rounded, color: Colors.cyanAccent, size: 20),
            tooltip: "Video Call",
            onPressed: _startVideoCall,
          ),
        ],
      ),
      body: Stack(
        children: [
          Column(
            children: [
              Expanded(
                child: ListView.builder(
                  controller: _scrollController,
                  itemCount: _messages.length,
                  padding: const EdgeInsets.only(top: 20, bottom: 20),
                  itemBuilder: (context, index) {
                    final msg = _messages[index];
                    if (msg is MessageModel) {
                      return GlassmorphicBubble(
                        content: msg.content,
                        isMe: msg.isMe,
                        timestamp: msg.timestamp,
                        status: msg.status,
                      );
                    } else if (msg is FileTransferProgress) {
                      return FileTransferBubble(
                        progress: msg,
                        isMe: msg.isOutgoing,
                      );
                    }
                    return const SizedBox.shrink();
                  },
                ),
              ),
              _buildInputArea(),
            ],
          ),
          Positioned(
            top: 20,
            left: 0,
            right: 0,
            child: Center(
              child: RewardsHUD(relayedBytes: _relayedBytes, solRewards: _solRewards),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildInputArea() {
    return Container(
      padding: const EdgeInsets.all(20),
      decoration: const BoxDecoration(
        color: Colors.black45,
        border: Border(top: BorderSide(color: Colors.white10, width: 0.5)),
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
                style: const TextStyle(color: Colors.white, fontFamily: 'monospace'),
                decoration: InputDecoration(
                  hintText: "ENTER ENCRYPTED PAYLOAD...",
                  hintStyle: const TextStyle(color: Colors.white24, fontSize: 12),
                  filled: true,
                  fillColor: Colors.white12,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                    borderSide: BorderSide.none,
                  ),
                  contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
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

class MessageModel {
  final String content;
  final bool isMe;
  final DateTime timestamp;
  int status; // 0=Sent, 1=Delivered, 2=Read
  final String? msgId;

  MessageModel({
    required this.content,
    required this.isMe,
    required this.timestamp,
    this.status = 0,
    this.msgId,
  });
}
