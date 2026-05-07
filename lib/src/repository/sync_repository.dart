import 'dart:async';
import 'package:flutter/foundation.dart';
import '../native/introvert_client.dart';

/// Explicit synchronization phases for state tracking.
enum SyncPhase {
  idle,
  generatingKeys,
  searchingPeer,
  syncingData,
  syncComplete,
  error
}

/// A production-grade repository orchestrating high-scale FFI synchronization tasks.
class SyncRepository {
  final IntrovertClient _client;

  SyncRepository({IntrovertClient? client})
      : _client = client ?? IntrovertClient();

  /// Initializes the local SQLCipher node via the FFI bridge.
  Future<bool> initializeLocalNode(Uint8List seed, String storagePath) async {
    try {
      _client.startEngine(seed, storagePath);
      return true;
    } catch (e) {
      debugPrint('Error initializing local node: $e');
      return false;
    }
  }

  /// Appends a new message to the local encrypted logs.
  Future<bool> logMessage(String peerId, String content) async {
    try {
      await _client.storeMessage(peerId, content);
      return true;
    } catch (e) {
      debugPrint('Error logging message: $e');
      return false;
    }
  }
}

/// Zero-jank state management for the synchronization pipeline.
class SyncStateNotifier extends ChangeNotifier {
  final SyncRepository repository;
  
  SyncPhase _phase = SyncPhase.idle;
  String? _errorMessage;

  SyncStateNotifier(this.repository);

  SyncPhase get phase => _phase;
  String? get errorMessage => _errorMessage;

  void setPhase(SyncPhase newPhase) {
    _phase = newPhase;
    notifyListeners();
  }

  void reset() {
    _phase = SyncPhase.idle;
    _errorMessage = null;
    notifyListeners();
  }
}
