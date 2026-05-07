import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import '../native/introvert_client.dart';

/// Hardware-Accelerated Native Video Renderer.
/// Bridges Rust WebRTC frames to Flutter's Texture primitive.
class NativeVideoRenderer extends StatefulWidget {
  final String peerId;
  const NativeVideoRenderer({required this.peerId, super.key});

  @override
  State<NativeVideoRenderer> createState() => _NativeVideoRendererState();
}

class _NativeVideoRendererState extends State<NativeVideoRenderer> {
  int? _textureId;
  bool _isInitialized = false;
  StreamSubscription<MediaFrameEvent>? _mediaSubscription;

  @override
  void initState() {
    super.initState();
    _initializeRenderer();
  }

  Future<void> _initializeRenderer() async {
    // Register texture via FFI (Mocked for now)
    setState(() {
      _textureId = 0; 
      _isInitialized = true;
    });

    // Register media frame callback
    final client = IntrovertClient();
    client.startNetwork();
    _mediaSubscription = client.mediaStream.listen((event) {
      if (_textureId != null) {
        // In production: IntrovertClient().updateTexture(_textureId!, event.payload, event.payloadLen);

        // Reclaim memory immediately after texture handoff
        client.freeBinary(event.basePtr, event.baseLen);
      }
    });
  }

  @override
  void dispose() {
    _mediaSubscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!_isInitialized || _textureId == null) {
      return const Center(
        child: CircularProgressIndicator(color: Colors.cyanAccent),
      );
    }

    return Container(
      color: Colors.black,
      child: Stack(
        children: [
          Center(
            child: AspectRatio(
              aspectRatio: 16 / 9,
              child: Texture(textureId: _textureId!),
            ),
          ),
          Positioned(
            bottom: 16,
            left: 16,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              decoration: BoxDecoration(
                color: Colors.black54,
                borderRadius: BorderRadius.circular(4),
              ),
              child: Row(
                children: [
                  const Icon(Icons.security, color: Colors.greenAccent, size: 14),
                  const SizedBox(width: 4),
                  Text(
                    "E2EE STREAM: ${widget.peerId.substring(0, 8)}...",
                    style: const TextStyle(
                      color: Colors.white70,
                      fontSize: 10,
                      fontFamily: 'monospace',
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
