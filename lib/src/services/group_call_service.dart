import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';
import '../native/introvert_client.dart';
import 'network_quality_service.dart';

/// Manages a full-mesh group WebRTC call.
///
/// Architecture (WhatsApp-style):
/// - Caller sends a [GROUP_CALL_INVITE] group message with call_id and members
/// - Each participant establishes individual 1:1 WebRTC connections to every other participant
/// - Late joiners see the ongoing call notification and can join
/// - When someone leaves, a [GROUP_CALL_LEAVE] message notifies others
///
/// Signal flow per peer:
/// createOffer → Rust forwards via mesh → Peer: setRemoteDesc → createAnswer → Rust returns → Connected
class GroupCallService extends ChangeNotifier {
  static GroupCallService? _instance;
  static GroupCallService get instance => _instance ??= GroupCallService._();

  GroupCallService._() {
    _networkSub = IntrovertClient().networkStream.listen(_handleNetworkEvent);
  }

  // --- State ---
  String? _callId;
  String? _groupId;
  int _mediaType = 2; // 0=audio, 2=video+audio
  bool _isInitiator = false;
  GroupCallState _callState = GroupCallState.idle;

  // Local media
  MediaStream? _localStream;
  RTCVideoRenderer? _localRenderer;

  // Per-peer connections: peerId -> PeerCallState
  final Map<String, PeerCallState> _peers = {};

  // Call timers
  DateTime? _callStartTime;
  Timer? _durationTimer;
  Duration _callDuration = Duration.zero;

  StreamSubscription? _networkSub;
  StreamSubscription? _qualitySub;

  // Adaptive quality state
  bool _wasDowngraded = false;
  bool _isNetworkPoor = false;

  // --- Public getters ---
  String? get callId => _callId;
  String? get groupId => _groupId;
  bool get isInitiator => _isInitiator;
  GroupCallState get callState => _callState;
  RTCVideoRenderer? get localRenderer => _localRenderer;
  MediaStream? get localStream => _localStream;
  Duration get callDuration => _callDuration;
  bool get isCallActive => _callState == GroupCallState.connected || _callState == GroupCallState.connecting;
  int get mediaType => _mediaType;
  DateTime? get callStartTime => _callStartTime;

  Map<String, PeerCallState> get peers => Map.unmodifiable(_peers);
  bool get wasDowngraded => _wasDowngraded;
  bool get isNetworkPoor => _isNetworkPoor;

  List<String> get connectedPeerIds =>
      _peers.entries.where((e) => e.value.state == PeerState.connected).map((e) => e.key).toList();

  List<String> get allParticipantIds {
    final ids = <String>{};
    if (_isInitiator) ids.add(IntrovertClient().localPeerId ?? '');
    for (final pid in _peers.keys) {
      ids.add(pid);
    }
    return ids.where((id) => id.isNotEmpty).toList();
  }

  // Callbacks
  Function()? onCallConnected;
  Function()? onCallEnded;
  Function(String peerId)? onPeerJoined;
  Function(String peerId)? onPeerLeft;
  Function(String reason)? onCallError;
  Function(String message)? onQualityWarning;

  /// Check if the network is suitable for a group call before starting.
  Future<PreCallCheckResult> checkNetworkForGroupCall(bool wantsVideo) async {
    return NetworkQualityService.instance.checkNetworkForCall(wantsVideo);
  }

  // --- Public methods ---

  /// Initialize local renderers
  Future<void> initialize() async {
    _localRenderer = RTCVideoRenderer();
    await _localRenderer!.initialize();
  }

  /// Get a remote renderer for a specific peer
  RTCVideoRenderer? getRemoteRenderer(String peerId) {
    return _peers[peerId]?.renderer;
  }

  /// Get display info for a peer (name, avatar)
  Map<String, String?> getPeerDisplayInfo(String peerId) {
    final contacts = IntrovertClient().getContacts();
    for (var c in contacts) {
      if (c['peer_id'] == peerId) {
        return {
          'name': c['alias'] ?? c['global_name'] ?? peerId.substring(0, 8),
          'avatar': c['avatar'],
        };
      }
    }
    return {
      'name': peerId.length > 8 ? peerId.substring(0, 8) : peerId,
      'avatar': null,
    };
  }

  /// Initiate a group call from the chat screen.
  /// [groupId] - the group to call
  /// [memberIds] - list of member peer IDs to call (excluding self)
  /// [mediaType] - 0=audio only, 2=video+audio
  Future<void> initiateGroupCall(String groupId, List<String> memberIds, int mediaType) async {
    if (_callState != GroupCallState.idle) {
      debugPrint('[GroupCall] Call already active');
      return;
    }

    // Check network quality before starting
    final networkCheck = await checkNetworkForGroupCall(mediaType != 0);
    if (!networkCheck.allowed) {
      onCallError?.call(networkCheck.reason);
      return;
    }
    // Use suggested media type if network is poor
    mediaType = networkCheck.suggestedMediaType;

    _callId = 'gc_${DateTime.now().millisecondsSinceEpoch}';
    _groupId = groupId;
    _mediaType = mediaType;
    _isInitiator = true;
    _callStartTime = DateTime.now();
    _callState = GroupCallState.calling;
    notifyListeners();

    // Start call duration timer
    _durationTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_callStartTime != null) {
        _callDuration = DateTime.now().difference(_callStartTime!);
        notifyListeners();
      }
    });

    // Initialize local media
    await _setupLocalMedia();

    // Send group call invite message
    final invitePayload = "[GROUP_CALL_INVITE]:${json.encode({
      'call_id': _callId,
      'group_id': groupId,
      'media_type': mediaType,
      'caller_id': IntrovertClient().localPeerId,
      'members': memberIds,
      'timestamp': DateTime.now().millisecondsSinceEpoch,
    })}";
    IntrovertClient().sendGroupMessage(groupId, invitePayload);

    // Initiate WebRTC connections to each member
    for (final peerId in memberIds) {
      _initiatePeerConnection(peerId, mediaType);
    }

    // If no members, end call immediately
    if (memberIds.isEmpty) {
      _callState = GroupCallState.connected;
      notifyListeners();
    }
  }

  /// Join an existing group call (late joiner)
  Future<void> joinGroupCall(String callId, String groupId, List<String> existingMembers, int mediaType) async {
    if (_callState != GroupCallState.idle) {
      debugPrint('[GroupCall] Call already active');
      return;
    }

    _callId = callId;
    _groupId = groupId;
    _mediaType = mediaType;
    _isInitiator = false;
    _callStartTime = DateTime.now();
    _callState = GroupCallState.connecting;
    notifyListeners();

    _durationTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_callStartTime != null) {
        _callDuration = DateTime.now().difference(_callStartTime!);
        notifyListeners();
      }
    });

    await _setupLocalMedia();

    // Notify the group we're joining
    final joinPayload = "[GROUP_CALL_JOIN]:${json.encode({
      'call_id': _callId,
      'group_id': groupId,
      'peer_id': IntrovertClient().localPeerId,
      'media_type': mediaType,
    })}";
    IntrovertClient().sendGroupMessage(groupId, joinPayload);

    // Initiate WebRTC connections to all existing members
    for (final peerId in existingMembers) {
      if (peerId != IntrovertClient().localPeerId) {
        _initiatePeerConnection(peerId, mediaType);
      }
    }

    // If no existing members, mark as connected
    if (existingMembers.isEmpty || (existingMembers.length == 1 && existingMembers.first == IntrovertClient().localPeerId)) {
      _callState = GroupCallState.connected;
      notifyListeners();
    }
  }

  /// Accept an incoming group call invite (called from notification/dialog)
  Future<void> acceptGroupCall(String callId, String groupId, String callerId, int mediaType) async {
    if (_callState != GroupCallState.idle) {
      debugPrint('[GroupCall] Call already active');
      return;
    }

    _callId = callId;
    _groupId = groupId;
    _mediaType = mediaType;
    _isInitiator = false;
    _callStartTime = DateTime.now();
    _callState = GroupCallState.connecting;
    notifyListeners();

    _durationTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_callStartTime != null) {
        _callDuration = DateTime.now().difference(_callStartTime!);
        notifyListeners();
      }
    });

    await _setupLocalMedia();

    // Accept the caller's individual call via Rust FFI
    IntrovertClient().acceptCall(callerId, mediaType);

    // Notify the group we've joined
    final joinPayload = "[GROUP_CALL_JOIN]:${json.encode({
      'call_id': _callId,
      'group_id': groupId,
      'peer_id': IntrovertClient().localPeerId,
      'media_type': mediaType,
    })}";
    IntrovertClient().sendGroupMessage(groupId, joinPayload);
  }

  /// Leave the group call
  Future<void> leaveCall() async {
    if (_callState == GroupCallState.idle) return;

    final peerId = IntrovertClient().localPeerId;

    // Send leave message
    if (_groupId != null && _callId != null) {
      final leavePayload = "[GROUP_CALL_LEAVE]:${json.encode({
        'call_id': _callId,
        'group_id': _groupId,
        'peer_id': peerId,
      })}";
      IntrovertClient().sendGroupMessage(_groupId!, leavePayload);
    }

    // Terminate all peer connections
    for (final entry in _peers.entries) {
      await _terminatePeerConnection(entry.key, local: true);
    }

    _cleanup();
    onCallEnded?.call();
  }

  /// Hang up (same as leave for group calls)
  Future<void> hangUp() async => leaveCall();

  /// Toggle mute
  Future<void> toggleMute() async {
    _localStream?.getAudioTracks().forEach((track) {
      track.enabled = track.enabled; // toggle
    });
    // Actually toggle
    final audioTracks = _localStream?.getAudioTracks();
    if (audioTracks != null && audioTracks.isNotEmpty) {
      final currentEnabled = audioTracks.first.enabled;
      for (final track in audioTracks) {
        track.enabled = !currentEnabled;
      }
    }
    notifyListeners();
  }

  bool _isMuted = false;
  bool get isMuted => _isMuted;

  Future<void> setMuted(bool muted) async {
    _isMuted = muted;
    _localStream?.getAudioTracks().forEach((track) {
      track.enabled = !muted;
    });
    notifyListeners();
  }

  /// Toggle camera
  bool _isCameraOff = false;
  bool get isCameraOff => _isCameraOff;

  Future<void> toggleCamera() async {
    _isCameraOff = !_isCameraOff;
    _localStream?.getVideoTracks().forEach((track) {
      track.enabled = !_isCameraOff;
    });
    notifyListeners();
  }

  /// Switch front/back camera
  Future<void> switchCamera() async {
    final videoTracks = _localStream?.getVideoTracks();
    if (videoTracks != null && videoTracks.isNotEmpty) {
      await Helper.switchCamera(videoTracks.first);
    }
  }

  /// Toggle speaker
  bool _isSpeakerOn = true;
  bool get isSpeakerOn => _isSpeakerOn;

  Future<void> setSpeakerphone(bool on) async {
    _isSpeakerOn = on;
    await Helper.setSpeakerphoneOn(on);
    notifyListeners();
  }

  // --- Private methods ---

  Future<void> _setupLocalMedia() async {
    final constraints = _mediaType == 0
        ? {
            'audio': {
              'echoCancellation': true,
              'noiseSuppression': true,
              'autoGainControl': true,
            },
            'video': false,
          }
        : {
            'audio': {
              'echoCancellation': true,
              'noiseSuppression': true,
              'autoGainControl': true,
            },
            'video': {
              'facingMode': 'user',
              'width': {'ideal': 640},
              'height': {'ideal': 480},
              'frameRate': {'ideal': 24},
            },
          };

    _localStream = await navigator.mediaDevices.getUserMedia(constraints);
    if (_localRenderer != null) {
      _localRenderer!.srcObject = _localStream;
    }
    notifyListeners();
  }

  void _initiatePeerConnection(String peerId, int mediaType) {
    if (_peers.containsKey(peerId)) return;

    final peerState = PeerCallState(peerId: peerId);
    _peers[peerId] = peerState;
    notifyListeners();

    _createPeerConnectionFor(peerId, peerState, mediaType, isCaller: true);
  }

  Future<void> _createPeerConnectionFor(
    String peerId,
    PeerCallState peerState,
    int mediaType, {
    required bool isCaller,
  }) async {
    const iceConfig = {
      'iceServers': [
        {'urls': 'stun:stun.l.google.com:19302'},
        {'urls': 'stun:stun1.l.google.com:19302'},
      ],
      'sdpSemantics': 'unified-plan',
      'bundlePolicy': 'max-bundle',
      'rtcpMuxPolicy': 'require',
    };

    peerState.peerConnection = await createPeerConnection(iceConfig);

    // Add local tracks
    if (_localStream != null) {
      _localStream!.getTracks().forEach((track) {
        peerState.peerConnection!.addTrack(track, _localStream!);
      });
    }

    peerState.peerConnection!.onIceCandidate = (RTCIceCandidate candidate) {
      _sendSignalToPeer(peerId, {
        'type': 'candidate',
        'candidate': candidate.candidate,
        'sdpMid': candidate.sdpMid,
        'sdpMLineIndex': candidate.sdpMLineIndex,
      });
    };

    peerState.peerConnection!.onIceConnectionState = (RTCIceConnectionState state) {
      debugPrint('[GroupCall] ICE state for $peerId: $state');
      if (state == RTCIceConnectionState.RTCIceConnectionStateConnected ||
          state == RTCIceConnectionState.RTCIceConnectionStateCompleted) {
        peerState.state = PeerState.connected;
        _checkAllConnected();
        _startGroupQualityMonitoring();
        notifyListeners();
        onPeerJoined?.call(peerId);
      } else if (state == RTCIceConnectionState.RTCIceConnectionStateFailed ||
          state == RTCIceConnectionState.RTCIceConnectionStateDisconnected) {
        peerState.state = PeerState.disconnected;
        onPeerLeft?.call(peerId);
        notifyListeners();
        _tryReconnectPeer(peerId);
      }
    };

    peerState.peerConnection!.onTrack = (RTCTrackEvent event) {
      debugPrint('[GroupCall] Remote track from $peerId: ${event.track.kind}');
      if (event.streams.isNotEmpty) {
        peerState.remoteStream = event.streams.first;
        if (peerState.renderer != null) {
          peerState.renderer!.srcObject = peerState.remoteStream;
        }
        notifyListeners();
      }
    };

    peerState.peerConnection!.onConnectionState = (RTCPeerConnectionState state) {
      debugPrint('[GroupCall] Connection state for $peerId: $state');
      if (state == RTCPeerConnectionState.RTCPeerConnectionStateFailed) {
        _tryReconnectPeer(peerId);
      }
    };

    if (isCaller) {
      // Create and send offer
      final offer = await peerState.peerConnection!.createOffer({
        'offerToReceiveAudio': true,
        'offerToReceiveVideo': mediaType != 0,
      });
      await peerState.peerConnection!.setLocalDescription(offer);

      _sendSignalToPeer(peerId, {
        'type': 'offer',
        'sdp': offer.sdp,
        'media_type': mediaType,
      });
    }
    // If callee, offer will arrive via signaling
  }

  /// Start monitoring group call quality across all peer connections.
  void _startGroupQualityMonitoring() {
    final connections = _peers.values
        .where((p) => p.peerConnection != null && p.state == PeerState.connected)
        .map((p) => p.peerConnection!)
        .toList();

    if (connections.isEmpty) return;

    NetworkQualityService.instance.startGroupMonitoring(connections);
    _qualitySub?.cancel();
    _qualitySub = NetworkQualityService.instance.eventStream.listen((event) {
      if (_callState == GroupCallState.idle) return;

      debugPrint('[GroupCall] Quality changed: ${event.previousQuality} → ${event.currentQuality}');

      if (event.shouldDowngradeVideo && _mediaType != 0) {
        _downgradeToAudio('Network quality degraded (bitrate: ${event.bitrate.round()} kbps, loss: ${event.packetLoss.round()}%)');
      }

      if (event.currentQuality == NetworkQuality.critical) {
        onCallError?.call('Connection quality critically low. Leaving call.');
        leaveCall();
      }
    });
  }

  /// Downgrade from video to audio during an active group call.
  void _downgradeToAudio(String reason) {
    if (_mediaType == 0) return;

    debugPrint('[GroupCall] Downgrading to audio: $reason');
    _wasDowngraded = true;
    _mediaType = 0;

    // Turn off video tracks
    _localStream?.getVideoTracks().forEach((track) {
      track.enabled = false;
    });

    // Notify all peers about downgrade
    for (final peerId in _peers.keys) {
      _sendSignalToPeer(peerId, {
        'type': 'quality_downgrade',
        'reason': reason,
      });
    }

    onQualityWarning?.call('Video disabled: $reason. Switched to audio only.');
    notifyListeners();
  }

  void _tryReconnectPeer(String peerId) {
    final peerState = _peers[peerId];
    if (peerState == null || peerState.state == PeerState.reconnecting) return;

    peerState.state = PeerState.reconnecting;
    peerState.reconnectAttempts++;
    notifyListeners();

    if (peerState.reconnectAttempts > 5) {
      _removePeer(peerId);
      return;
    }

    // Retry with exponential backoff
    final delay = Duration(seconds: peerState.reconnectAttempts * 2);
    Future.delayed(delay, () async {
      if (_callState == GroupCallState.idle) return;
      debugPrint('[GroupCall] Reconnecting to $peerId (attempt ${peerState.reconnectAttempts})');
      // Close old connection
      await peerState.peerConnection?.close();
      peerState.peerConnection = null;
      peerState.renderer?.srcObject = null;
      peerState.state = PeerState.connecting;
      notifyListeners();
      _createPeerConnectionFor(peerId, peerState, _mediaType, isCaller: true);
    });
  }

  void _checkAllConnected() {
    if (_callState == GroupCallState.calling || _callState == GroupCallState.connecting) {
      if (_peers.isEmpty || _peers.values.any((p) => p.state == PeerState.connected)) {
        _callState = GroupCallState.connected;
        notifyListeners();
        onCallConnected?.call();
      }
    }
  }

  void _handleNetworkEvent(NetworkEvent event) {
    // Event 15: WebRTC signaling
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
        debugPrint('[GroupCall] Error parsing Event 15: $e');
      }
    }
    // Event 16: Call ended/rejected
    else if (event.type == 16) {
      try {
        final peerId = utf8.decode(event.data);
        if (_peers.containsKey(peerId)) {
          _removePeer(peerId);
        }
      } catch (e) {
        debugPrint('[GroupCall] Error handling call end: $e');
      }
    }
  }

  void _handleIncomingSignalFromPeer(String peerId, String jsonStr) async {
    if (_callState == GroupCallState.idle) return;

    try {
      final json = jsonDecode(jsonStr) as Map<String, dynamic>;
      final type = json['type'] as String?;

      if (type == 'offer') {
        await _handleOfferFromPeer(peerId, json);
      } else if (type == 'answer') {
        await _handleAnswerFromPeer(peerId, json['sdp'] as String);
      } else if (type == 'candidate') {
        await _handleCandidateFromPeer(
          peerId,
          json['candidate'] as String?,
          json['sdpMid'] as String?,
          json['sdpMLineIndex'] as int?,
        );
      }
    } catch (e) {
      debugPrint('[GroupCall] Error handling signal from $peerId: $e');
    }
  }

  Future<void> _handleOfferFromPeer(String peerId, Map<String, dynamic> json) async {
    var peerState = _peers[peerId];
    if (peerState == null) {
      // Late joiner or new peer - accept their offer
      peerState = PeerCallState(peerId: peerId, state: PeerState.connecting);
      _peers[peerId] = peerState;
      await _createPeerConnectionFor(peerId, peerState, json['media_type'] as int? ?? 2, isCaller: false);
    }

    if (peerState.peerConnection == null) return;

    await peerState.peerConnection!.setRemoteDescription(
      RTCSessionDescription(json['sdp'] as String, 'offer'),
    );

    // Drain pending candidates
    for (final c in peerState.pendingCandidates) {
      try {
        await peerState.peerConnection!.addCandidate(
          RTCIceCandidate(c['candidate'] as String, c['sdpMid'] as String?, c['sdpMLineIndex'] as int?),
        );
      } catch (_) {}
    }
    peerState.pendingCandidates.clear();

    final answer = await peerState.peerConnection!.createAnswer();
    await peerState.peerConnection!.setLocalDescription(answer);

    _sendSignalToPeer(peerId, {
      'type': 'answer',
      'sdp': answer.sdp,
    });
  }

  Future<void> _handleAnswerFromPeer(String peerId, String sdp) async {
    final peerState = _peers[peerId];
    if (peerState == null || peerState.peerConnection == null) return;

    await peerState.peerConnection!.setRemoteDescription(
      RTCSessionDescription(sdp, 'answer'),
    );

    // Drain pending candidates
    for (final c in peerState.pendingCandidates) {
      try {
        await peerState.peerConnection!.addCandidate(
          RTCIceCandidate(c['candidate'] as String, c['sdpMid'] as String?, c['sdpMLineIndex'] as int?),
        );
      } catch (_) {}
    }
    peerState.pendingCandidates.clear();
  }

  Future<void> _handleCandidateFromPeer(
    String peerId,
    String? candidate,
    String? sdpMid,
    int? sdpMLineIndex,
  ) async {
    if (candidate == null) return;
    final peerState = _peers[peerId];
    if (peerState == null) return;

    if (peerState.peerConnection == null) {
      peerState.pendingCandidates.add({
        'candidate': candidate,
        'sdpMid': sdpMid,
        'sdpMLineIndex': sdpMLineIndex,
      });
      return;
    }

    final desc = await peerState.peerConnection!.getRemoteDescription();
    if (desc == null) {
      peerState.pendingCandidates.add({
        'candidate': candidate,
        'sdpMid': sdpMid,
        'sdpMLineIndex': sdpMLineIndex,
      });
      return;
    }

    try {
      await peerState.peerConnection!.addCandidate(
        RTCIceCandidate(candidate, sdpMid, sdpMLineIndex),
      );
    } catch (e) {
      debugPrint('[GroupCall] Error adding ICE candidate from $peerId: $e');
    }
  }

  void _sendSignalToPeer(String peerId, Map<String, dynamic> signal) {
    try {
      final bytes = Uint8List.fromList(utf8.encode(jsonEncode(signal)));
      IntrovertClient().sendWebRtcSignal(peerId, bytes);
    } catch (e) {
      debugPrint('[GroupCall] Error sending signal to $peerId: $e');
    }
  }

  Future<void> _terminatePeerConnection(String peerId, {required bool local}) async {
    final peerState = _peers[peerId];
    if (peerState == null) return;

    if (local) {
      try {
        IntrovertClient().closeWebRtc(peerId);
      } catch (e) {
        debugPrint('[GroupCall] Error closing webrtc to $peerId: $e');
      }
    }

    await peerState.peerConnection?.close();
    peerState.renderer?.srcObject = null;
    peerState.remoteStream = null;
  }

  void _removePeer(String peerId) {
    _terminatePeerConnection(peerId, local: false);
    _peers.remove(peerId);
    notifyListeners();

    onPeerLeft?.call(peerId);

    // If no peers left and not initiator, end call
    if (_peers.isEmpty && !_isInitiator) {
      _cleanup();
      onCallEnded?.call();
    }
  }

  void _cleanup() {
    _callState = GroupCallState.idle;
    _callId = null;
    _groupId = null;
    _isInitiator = false;
    _isMuted = false;
    _isCameraOff = false;
    _isSpeakerOn = true;
    _callStartTime = null;
    _callDuration = Duration.zero;
    _durationTimer?.cancel();
    _wasDowngraded = false;
    _isNetworkPoor = false;

    // Stop quality monitoring
    NetworkQualityService.instance.stopMonitoring();
    _qualitySub?.cancel();
    _qualitySub = null;

    _localStream?.getTracks().forEach((track) => track.stop());
    _localStream?.dispose();
    _localStream = null;

    if (_localRenderer != null) {
      _localRenderer!.srcObject = null;
    }

    for (final peerState in _peers.values) {
      peerState.peerConnection?.close();
      peerState.renderer?.dispose();
      peerState.renderer?.srcObject = null;
    }
    _peers.clear();

    notifyListeners();
  }

  @override
  void dispose() {
    _networkSub?.cancel();
    _qualitySub?.cancel();
    NetworkQualityService.instance.stopMonitoring();
    _localRenderer?.dispose();
    _cleanup();
    super.dispose();
  }
}

enum GroupCallState { idle, calling, connecting, connected }

enum PeerState { connecting, connected, disconnected, reconnecting }

class PeerCallState {
  final String peerId;
  PeerState state;
  RTCPeerConnection? peerConnection;
  RTCVideoRenderer? renderer;
  MediaStream? remoteStream;
  final List<Map<String, dynamic>> pendingCandidates = [];
  int reconnectAttempts = 0;

  PeerCallState({
    required this.peerId,
    this.state = PeerState.connecting,
  }) {
    renderer = RTCVideoRenderer();
    renderer!.initialize();
  }
}
