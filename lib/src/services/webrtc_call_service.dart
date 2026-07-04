import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';
import '../native/introvert_client.dart';
import 'network_quality_service.dart';

/// Manages the full WebRTC call lifecycle using flutter_webrtc for native AV.
/// 
/// Architecture:
/// - flutter_webrtc handles: mic/camera capture, codec negotiation, ICE, DTLS/SRTP, playback
/// - Rust (libp2p) handles: peer discovery, E2EE relay routing, SDP/ICE forwarding via mesh
///
/// Signal flow:
/// Caller: createOffer → Rust forwards via mesh → Callee: setRemoteDesc → createAnswer → Rust returns → Caller: setRemoteDesc → ICE exchange → Connected
class WebRtcCallService extends ChangeNotifier {
  static WebRtcCallService? _instance;
  static WebRtcCallService get instance => _instance ??= WebRtcCallService._();

  WebRtcCallService._() {
    // Start listening to signaling events immediately to prevent lost events
    _networkSub = IntrovertClient().networkStream.listen(_handleNetworkEvent);
  }

  RTCPeerConnection? _peerConnection;
  MediaStream? _localStream;
  MediaStream? _remoteStream;
  RTCVideoRenderer? _localRenderer;
  RTCVideoRenderer? _remoteRenderer;

  String? _currentPeerId;
  bool _isCallActive = false;
  bool _isMuted = false;
  bool _isCameraOff = false;
  bool _isSpeakerOn = true;
  CallState _callState = CallState.idle;
  int _reconnectAttempts = 0;
  static const int _maxReconnectAttempts = 3;

  StreamSubscription? _networkSub;
  final List<Map<String, dynamic>> _pendingCandidates = [];

  // Adaptive quality state
  int _currentMediaType = 2; // 2=video+audio, 0=audio only
  bool _wasDowngraded = false;
  StreamSubscription? _qualitySub;

  // Public state getters
  bool get isCallActive => _isCallActive;
  bool get isMuted => _isMuted;
  bool get isCameraOff => _isCameraOff;
  bool get isSpeakerOn => _isSpeakerOn;
  CallState get callState => _callState;
  String? get currentPeerId => _currentPeerId;
  RTCVideoRenderer? get localRenderer => _localRenderer;
  RTCVideoRenderer? get remoteRenderer => _remoteRenderer;
  int get currentMediaType => _currentMediaType;
  bool get wasDowngraded => _wasDowngraded;

  // Callbacks
  Function()? onCallConnected;
  Function()? onCallEnded;
  Function(String reason)? onCallError;
  Function(String message)? onQualityWarning;

  static const Map<String, dynamic> _iceConfig = {
    'iceServers': [
      {'urls': 'stun:stun.l.google.com:19302'},
      {'urls': 'stun:stun1.l.google.com:19302'},
      {'urls': 'stun:stun2.l.google.com:19302'},
    ],
    'sdpSemantics': 'unified-plan',
    'bundlePolicy': 'max-bundle',
    'rtcpMuxPolicy': 'require',
  };

  static const Map<String, dynamic> _mediaConstraints = {
    'audio': {
      'echoCancellation': true,
      'noiseSuppression': true,
      'autoGainControl': true,
      'sampleRate': 48000,
    },
    'video': {
      'facingMode': 'user',
      'width': {'ideal': 1280},
      'height': {'ideal': 720},
      'frameRate': {'ideal': 30},
    },
  };

  static const Map<String, dynamic> _audioOnlyConstraints = {
    'audio': {
      'echoCancellation': true,
      'noiseSuppression': true,
      'autoGainControl': true,
      'sampleRate': 48000,
    },
    'video': false,
  };

  /// Initialize renderers
  Future<void> initialize() async {
    _localRenderer = RTCVideoRenderer();
    _remoteRenderer = RTCVideoRenderer();
    await _localRenderer!.initialize();
    await _remoteRenderer!.initialize();
  }

  /// Check if the network is suitable for a call before starting.
  /// Returns the check result with reason if not suitable.
  Future<PreCallCheckResult> checkNetworkBeforeCall(bool wantsVideo) async {
    return NetworkQualityService.instance.checkNetworkForCall(wantsVideo);
  }

  /// Start monitoring call quality after connection is established.
  void _startQualityMonitoring() {
    if (_peerConnection == null) return;

    NetworkQualityService.instance.startMonitoring(_peerConnection!);
    _qualitySub = NetworkQualityService.instance.eventStream.listen((event) {
      debugPrint('[WebRTC] Quality changed: ${event.previousQuality} → ${event.currentQuality}');

      // Record quality sample for Intro-Claw
      try {
        final client = IntrovertClient();
        final isRelayed = false; // Will be enhanced with actual relay detection
        client.voipRecordSample(
          event.rtt.round(),
          event.packetLoss,
          0, // jitter not available from this source
          event.bitrate.round(),
          isRelayed,
          'opus',
        );
      } catch (_) {}

      if (event.shouldDowngradeVideo && _currentMediaType != 0) {
        _downgradeToAudio('Network quality degraded (bitrate: ${event.bitrate.round()} kbps, loss: ${event.packetLoss.round()}%)');
      }

      if (event.currentQuality == NetworkQuality.critical) {
        onCallError?.call('Connection quality critically low. Call will be terminated.');
        hangUp();
      }
    });
  }

  /// Downgrade from video to audio during an active call.
  Future<void> _downgradeToAudio(String reason) async {
    if (_currentMediaType == 0 || _peerConnection == null) return;

    debugPrint('[WebRTC] Downgrading to audio: $reason');
    _wasDowngraded = true;
    _currentMediaType = 0;

    // Turn off video tracks
    _localStream?.getVideoTracks().forEach((track) {
      track.enabled = false;
    });
    _isCameraOff = true;

    // Send notification to remote peer
    _sendSignal(_currentPeerId!, {
      'type': 'quality_downgrade',
      'reason': reason,
    });

    onQualityWarning?.call('Video disabled: $reason. Switched to audio only.');
    notifyListeners();
  }

  /// Try to upgrade back to video if quality improves.
  Future<void> tryUpgradeToVideo() async {
    if (_currentMediaType != 0 || _peerConnection == null || _wasDowngraded) return;

    final quality = NetworkQualityService.instance.currentQuality;
    if (quality == NetworkQuality.excellent || quality == NetworkQuality.good) {
      _currentMediaType = 2;
      _isCameraOff = false;

      _localStream?.getVideoTracks().forEach((track) {
        track.enabled = true;
      });

      onQualityWarning?.call('Network quality improved. Video restored.');
      notifyListeners();
    }
  }

  void _handleNetworkEvent(NetworkEvent event) {
    // Event 15: Incoming flutter_webrtc signal (SDP offer/answer or ICE candidate)
    // Format: [peer_id_len: u8][peer_id_bytes][json_bytes]
    if (event.type == 15) {
      try {
        final data = event.data;
        if (data.isEmpty) return;
        final peerIdLen = data[0];
        if (data.length < 1 + peerIdLen) return;
        final peerId = utf8.decode(data.sublist(1, 1 + peerIdLen));
        final json = utf8.decode(data.sublist(1 + peerIdLen));
        _handleIncomingSignalFromPeer(peerId, json);
      } catch (e) {
        debugPrint('[WebRTC] Error parsing Event 15: $e');
      }
    }
    // Event 16: Remote peer ended/rejected the call
    else if (event.type == 16) {
      try {
        final peerId = utf8.decode(event.data);
        if (peerId == _currentPeerId) {
          _terminateCall(local: false);
        }
      } catch (e) {
        debugPrint('[WebRTC] Error handling call end event: $e');
      }
    }
  }

  void _handleIncomingSignalFromPeer(String peerId, String jsonStr) async {
    try {
      final json = jsonDecode(jsonStr) as Map<String, dynamic>;
      final type = json['type'] as String?;

      if (type == 'offer') {
        await _handleOffer(json['sdp'] as String, peerId, json['media_type'] as int? ?? 2);
      } else if (type == 'answer') {
        await _handleAnswer(json['sdp'] as String);
      } else if (type == 'candidate') {
        await _handleCandidate(
          json['candidate'] as String?,
          json['sdpMid'] as String?,
          json['sdpMLineIndex'] as int?,
        );
      } else if (type == 'quality_downgrade') {
        // Remote peer downgraded to audio
        final reason = json['reason'] as String? ?? 'Remote peer downgraded';
        debugPrint('[WebRTC] Remote peer downgraded: $reason');
        onQualityWarning?.call('Remote peer switched to audio: $reason');
      }
    } catch (e) {
      debugPrint('[WebRTC] Error handling signal from $peerId: $e');
    }
  }

  /// Caller: initiate a call to a peer
  Future<void> initiateCall(String peerId, int mediaType) async {
    if (_isCallActive) {
      debugPrint('[WebRTC] Call already active');
      return;
    }

    _currentPeerId = peerId;
    _callState = CallState.calling;
    notifyListeners();

    try {
      await _setupPeerConnection();
      await _acquireLocalMedia(mediaType);
      await _createAndSendOffer(peerId, mediaType);
    } catch (e) {
      debugPrint('[WebRTC] Error initiating call: $e');
      onCallError?.call('Failed to start call: $e');
      await _terminateCall(local: true);
    }
  }

  /// Callee: accept an incoming call
  Future<void> acceptCall(String peerId, int mediaType) async {
    _currentPeerId = peerId;
    _callState = CallState.connecting;
    notifyListeners();

    try {
      await _setupPeerConnection();
      await _acquireLocalMedia(mediaType);
      // Answer will be created when the offer arrives via Event 15
      // If offer already arrived (stored in _pendingOffer), handle it now
      if (_pendingOffer != null) {
        await _handleOffer(_pendingOffer!['sdp']!, peerId, mediaType);
        _pendingOffer = null;
      }
    } catch (e) {
      debugPrint('[WebRTC] Error accepting call: $e');
      onCallError?.call('Failed to accept call: $e');
      await _terminateCall(local: true);
    }
  }

  Map<String, String>? _pendingOffer;

  Future<void> _setupPeerConnection() async {
    _peerConnection = await createPeerConnection(_iceConfig);

    _peerConnection!.onIceCandidate = (RTCIceCandidate candidate) {
      debugPrint('[WebRTC] Local ICE candidate: ${candidate.candidate}');
      _sendSignal(_currentPeerId!, {
        'type': 'candidate',
        'candidate': candidate.candidate,
        'sdpMid': candidate.sdpMid,
        'sdpMLineIndex': candidate.sdpMLineIndex,
        'peer_id': _currentPeerId,
      });
    };

    _peerConnection!.onIceConnectionState = (RTCIceConnectionState state) {
      debugPrint('[WebRTC] ICE state: $state');
      if (state == RTCIceConnectionState.RTCIceConnectionStateConnected ||
          state == RTCIceConnectionState.RTCIceConnectionStateCompleted) {
        _isCallActive = true;
        _callState = CallState.connected;
        _reconnectAttempts = 0;
        _startQualityMonitoring();
        // Notify Intro-Claw of call start
        try {
          IntrovertClient().voipStartCall(_currentPeerId ?? '', _currentMediaType == 2);
        } catch (_) {}
        notifyListeners();
        onCallConnected?.call();
      } else if (state == RTCIceConnectionState.RTCIceConnectionStateFailed ||
          state == RTCIceConnectionState.RTCIceConnectionStateDisconnected) {
        debugPrint('[WebRTC] ICE failed/disconnected — attempting reconnect (${_reconnectAttempts + 1}/$_maxReconnectAttempts)');
        _reconnectAttempts++;
        if (_reconnectAttempts <= _maxReconnectAttempts && _currentPeerId != null) {
          _callState = CallState.connecting;
          notifyListeners();
          final peerId = _currentPeerId!;
          final mediaType = _currentMediaType;
          final delay = Duration(seconds: _reconnectAttempts * 2);
          Future.delayed(delay, () async {
            if (_callState == CallState.idle) return;
            debugPrint('[WebRTC] Reconnecting to $peerId...');
            await _terminateCall(local: false);
            _reconnectAttempts = 0;
            _currentPeerId = peerId;
            _callState = CallState.calling;
            notifyListeners();
            try {
              await _setupPeerConnection();
              await _acquireLocalMedia(mediaType);
              await _createAndSendOffer(peerId, mediaType);
            } catch (e) {
              debugPrint('[WebRTC] Reconnect failed: $e');
              _terminateCall(local: true);
            }
          });
        } else {
          _terminateCall(local: true);
        }
      }
    };

    _peerConnection!.onTrack = (RTCTrackEvent event) {
      debugPrint('[WebRTC] Remote track received: ${event.track.kind}');
      if (event.streams.isNotEmpty) {
        _remoteStream = event.streams.first;
        if (_remoteRenderer != null) {
          _remoteRenderer!.srcObject = _remoteStream;
        }
        notifyListeners();
      }
    };

    _peerConnection!.onConnectionState = (RTCPeerConnectionState state) {
      debugPrint('[WebRTC] Connection state: $state');
      if (state == RTCPeerConnectionState.RTCPeerConnectionStateFailed ||
          state == RTCPeerConnectionState.RTCPeerConnectionStateClosed) {
        _terminateCall(local: false);
      }
    };
  }

  Future<void> _acquireLocalMedia(int mediaType) async {
    final constraints = mediaType == 0 ? _audioOnlyConstraints : _mediaConstraints;
    _localStream = await navigator.mediaDevices.getUserMedia(constraints);

    if (_localRenderer != null) {
      _localRenderer!.srcObject = _localStream;
    }

    // Add local tracks to peer connection
    _localStream!.getTracks().forEach((track) {
      _peerConnection!.addTrack(track, _localStream!);
    });

    notifyListeners();
  }

  Future<void> _createAndSendOffer(String peerId, int mediaType) async {
    final offer = await _peerConnection!.createOffer({
      'offerToReceiveAudio': true,
      'offerToReceiveVideo': mediaType != 0,
    });
    await _peerConnection!.setLocalDescription(offer);
    debugPrint('[WebRTC] Created offer, sending to peer...');

    _sendSignal(peerId, {
      'type': 'offer',
      'sdp': offer.sdp,
      'peer_id': peerId,
      'media_type': mediaType,
    });
  }

  Future<void> _drainPendingCandidates() async {
    if (_peerConnection == null) return;
    if (_pendingCandidates.isNotEmpty) {
      debugPrint('[WebRTC] Draining ${_pendingCandidates.length} pending candidates');
      for (final c in _pendingCandidates) {
        try {
          await _peerConnection!.addCandidate(
            RTCIceCandidate(
              c['candidate'] as String,
              c['sdpMid'] as String?,
              c['sdpMLineIndex'] as int?,
            ),
          );
        } catch (e) {
          debugPrint('[WebRTC] Error adding pending ICE candidate: $e');
        }
      }
      _pendingCandidates.clear();
    }
  }

  Future<void> _handleOffer(String sdp, String peerId, int mediaType) async {
    if (_peerConnection == null) {
      // Store for later if peer connection not ready yet
      _pendingOffer = {'sdp': sdp, 'peer_id': peerId};
      return;
    }

    await _peerConnection!.setRemoteDescription(
      RTCSessionDescription(sdp, 'offer'),
    );

    await _drainPendingCandidates();

    final answer = await _peerConnection!.createAnswer();
    await _peerConnection!.setLocalDescription(answer);

    _sendSignal(peerId, {
      'type': 'answer',
      'sdp': answer.sdp,
      'peer_id': peerId,
    });
  }

  Future<void> _handleAnswer(String sdp) async {
    if (_peerConnection == null) return;
    await _peerConnection!.setRemoteDescription(
      RTCSessionDescription(sdp, 'answer'),
    );
    await _drainPendingCandidates();
  }

  Future<void> _handleCandidate(String? candidate, String? sdpMid, int? sdpMLineIndex) async {
    if (candidate == null) return;
    
    // If peer connection or remote description is not set yet, buffer candidate
    if (_peerConnection == null) {
      _pendingCandidates.add({
        'candidate': candidate,
        'sdpMid': sdpMid,
        'sdpMLineIndex': sdpMLineIndex,
      });
      return;
    }
    
    final desc = await _peerConnection!.getRemoteDescription();
    if (desc == null) {
      _pendingCandidates.add({
        'candidate': candidate,
        'sdpMid': sdpMid,
        'sdpMLineIndex': sdpMLineIndex,
      });
      return;
    }

    try {
      await _peerConnection!.addCandidate(
        RTCIceCandidate(candidate, sdpMid, sdpMLineIndex),
      );
    } catch (e) {
      debugPrint('[WebRTC] Error adding ICE candidate: $e');
    }
  }

  /// Send a WebRTC signal through Rust's encrypted mesh
  void _sendSignal(String peerId, Map<String, dynamic> signal) {
    try {
      final bytes = Uint8List.fromList(utf8.encode(jsonEncode(signal)));
      IntrovertClient().sendWebRtcSignal(peerId, bytes);
    } catch (e) {
      debugPrint('[WebRTC] Error sending signal: $e');
    }
  }

  Future<void> toggleMute() async {
    _isMuted = !_isMuted;
    _localStream?.getAudioTracks().forEach((track) {
      track.enabled = !_isMuted;
    });
    notifyListeners();
  }

  Future<void> toggleCamera() async {
    _isCameraOff = !_isCameraOff;
    _localStream?.getVideoTracks().forEach((track) {
      track.enabled = !_isCameraOff;
    });
    notifyListeners();
  }

  Future<void> switchCamera() async {
    final videoTracks = _localStream?.getVideoTracks();
    if (videoTracks != null && videoTracks.isNotEmpty) {
      await Helper.switchCamera(videoTracks.first);
    }
  }

  Future<void> setSpeakerphone(bool on) async {
    _isSpeakerOn = on;
    await Helper.setSpeakerphoneOn(on);
    notifyListeners();
  }

  /// Hang up the call
  Future<void> hangUp() async {
    await _terminateCall(local: true);
  }

  Future<void> _terminateCall({required bool local}) async {
    if (_callState == CallState.idle) return;

    debugPrint('[WebRTC] Terminating call (local=$local)');
    _callState = CallState.idle;
    _isCallActive = false;

    // Notify Intro-Claw of call end
    try {
      IntrovertClient().voipEndCall();
    } catch (_) {}

    // Stop quality monitoring
    NetworkQualityService.instance.stopMonitoring();
    _qualitySub?.cancel();
    _qualitySub = null;

    if (local && _currentPeerId != null) {
      // Signal remote peer that we're ending
      try {
        IntrovertClient().closeWebRtc(_currentPeerId!);
      } catch (e) {
        debugPrint('[WebRTC] Error closing webrtc: $e');
      }
    }

    // Stop local media
    _localStream?.getTracks().forEach((track) => track.stop());
    await _localStream?.dispose();
    _localStream = null;

    // Close peer connection
    await _peerConnection?.close();
    _peerConnection = null;

    // Clear renderers
    if (_localRenderer != null) {
      _localRenderer!.srcObject = null;
    }
    if (_remoteRenderer != null) {
      _remoteRenderer!.srcObject = null;
    }

    _currentPeerId = null;
    _isMuted = false;
    _isCameraOff = false;
    _currentMediaType = 2;
    _wasDowngraded = false;
    _pendingOffer = null;
    _pendingCandidates.clear();

    notifyListeners();
    onCallEnded?.call();
  }

  @override
  void dispose() {
    _networkSub?.cancel();
    _qualitySub?.cancel();
    NetworkQualityService.instance.stopMonitoring();
    _localRenderer?.dispose();
    _remoteRenderer?.dispose();
    super.dispose();
  }
}

enum CallState { idle, calling, connecting, connected }
