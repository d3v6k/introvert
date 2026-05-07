import 'dart:async';
import 'package:flutter/material.dart';
import '../src/native/introvert_client.dart';
import '../src/ui/widgets/security_shield.dart';
import '../src/ui/widgets/rewards_hud.dart';
import '../blueprint_ui.dart';
import 'call_screen.dart';

class ChatScreen extends StatefulWidget {
  final String peerId;

  const ChatScreen({required this.peerId, super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final List<MessageModel> _messages = [];
  final IntrovertClient _client = IntrovertClient();
  
  bool _isE2eeActive = false;
  String _fingerprint = "LOOKUP...";
  int _relayedBytes = 0;
  double _solRewards = 0.0;
  String _status = "Offline";
  Color _statusColor = Colors.redAccent;

  late final StreamSubscription<NetworkEvent> _networkSubscription;
  StreamSubscription<MediaFrameEvent>? _mediaSubscription;

  @override
  void initState() {
    super.initState();
    _startNetworkDiscovery();
  }

  void _startNetworkDiscovery() {
    _client.startNetwork();
    _networkSubscription = _client.networkStream.listen((event) {
      if (event.type == 1) {
        // Kademlia/DHT Discovery Update
        setState(() {
          _fingerprint = widget.peerId.substring(widget.peerId.length - 8).toUpperCase();
        });
      } else if (event.type == 8) {
        // Event 8: Connection Status (0=Direct, 1=Relay, 2=Offline)
        final statusCode = event.data[0];
        setState(() {
          if (statusCode == 0) {
            _status = "Direct P2P";
            _statusColor = Colors.greenAccent;
          } else if (statusCode == 1) {
            _status = "Relay Active";
            _statusColor = Colors.orangeAccent;
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
        // Standard or Drained Message
        final msg = String.fromCharCodes(event.data);
        if (msg.startsWith("WEBRTC:")) return; // Ignore internal signaling
        
        setState(() {
          if (event.type == 2) _isE2eeActive = true; // Message reached core and was emitted
          _messages.add(MessageModel(
            content: msg,
            isMe: false,
            timestamp: DateTime.now(),
          ));
        });
        _scrollToBottom();
      }
    });

    _mediaSubscription = _client.mediaStream.listen((event) {
      // Chat screen doesn't render video directly now, navigate to CallScreen.
      // Free memory to prevent leaks.
      _client.freeBinary(event.basePtr, event.baseLen);
    });
  }

  @override
  void dispose() {
    _networkSubscription.cancel();
    _mediaSubscription?.cancel();
    _messageController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _sendMessage() async {
    final text = _messageController.text.trim();
    if (text.isEmpty) return;

    try {
      await _client.storeMessage(widget.peerId, text);
      await _client.sendMessage(widget.peerId, text);

      setState(() {
        _messages.add(MessageModel(
          content: text,
          isMe: true,
          timestamp: DateTime.now(),
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
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              "PEER: ${widget.peerId}",
              style: const TextStyle(fontSize: 12, fontFamily: 'monospace', color: Colors.cyanAccent),
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
                    return GlassmorphicBubble(
                      content: msg.content,
                      isMe: msg.isMe,
                    );
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

  MessageModel({
    required this.content,
    required this.isMe,
    required this.timestamp,
  });
}
