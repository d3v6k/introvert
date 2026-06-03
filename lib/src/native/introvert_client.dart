import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';

// --- Native C Signatures & Dart Mapping ---

final class FfiResult extends Struct {
  @Int32()
  external int code;
  external Pointer<Uint8> data;
  @Size()
  external int len;

  // Persistent native dummy for safe FFI fallbacks
  static FfiResult get dummy {
    _dummyPtr ??= calloc<FfiResult>()
      ..ref.code = -1
      ..ref.data = nullptr
      ..ref.len = 0;
    return _dummyPtr!.ref;
  }

  static FfiResult success() {
    final res = calloc<FfiResult>();
    res.ref.code = 0;
    res.ref.data = nullptr;
    res.ref.len = 0;
    final val = res.ref;
    calloc.free(res);
    return val;
  }
}

Pointer<FfiResult>? _dummyPtr;

final class MediaFrameHeader extends Struct {
  @Uint8()
  external int codec;
  @Uint32()
  external int width;
  @Uint32()
  external int height;
  @Uint64()
  external int timestamp;
}

typedef NativeNetworkCallback = Void Function(Int32 eventType, Pointer<Uint8> dataPtr, Size dataLen);
typedef NativeFfiCallback = Void Function(FfiResult result);
typedef NativeRewardCallback = Void Function(Int32 status, Pointer<Utf8> txSignature);

typedef IntrovertGenerateMnemonicC = Pointer<Utf8> Function();
typedef IntrovertGenerateMnemonicDart = Pointer<Utf8> Function();

typedef IntrovertFreeStringC = Void Function(Pointer<Utf8> s);
typedef IntrovertFreeStringDart = void Function(Pointer<Utf8> s);

typedef IntrovertFreeBinaryC = Void Function(Pointer<Uint8> ptr, Size len);
typedef IntrovertFreeBinaryDart = void Function(Pointer<Uint8> ptr, int len);

typedef IntrovertMnemonicToSeedC = FfiResult Function(Pointer<Utf8> phrase);
typedef IntrovertMnemonicToSeedDart = FfiResult Function(Pointer<Utf8> phrase);

typedef IntrovertDeriveIdentifiersC = FfiResult Function(Pointer<Uint8> seed);
typedef IntrovertDeriveIdentifiersDart = FfiResult Function(Pointer<Uint8> seed);

typedef IntrovertEngineStartC = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);
typedef IntrovertEngineStartDart = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);

typedef IntrovertEngineStopC = FfiResult Function();
typedef IntrovertEngineStopDart = FfiResult Function();

typedef IntrovertGetPeerIdC = Pointer<Utf8> Function();
typedef IntrovertGetPeerIdDart = Pointer<Utf8> Function();

typedef IntrovertNetworkStartC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback, Uint16 port, Bool relay, Uint32 maxConn, Uint64 liveness);
typedef IntrovertNetworkStartDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback, int port, bool relay, int maxConn, int liveness);

typedef IntrovertEconomyStartMonitoringC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);
typedef IntrovertEconomyStartMonitoringDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);

typedef IntrovertNetworkSendMessageC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkSendMessageDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertNetworkInitiateWebRtcC = FfiResult Function(Pointer<Utf8> peerId, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkInitiateWebRtcDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertAddAddressC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);
typedef IntrovertAddAddressDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);

typedef IntrovertClaimRewardsAsyncC = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);
typedef IntrovertClaimRewardsAsyncDart = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);

typedef IntrovertStoreMessageAsyncC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Bool isMe, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertStoreMessageAsyncDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, bool isMe, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertStorageGetMessagesC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertStorageGetMessagesDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertEstablishSecureSessionC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertEstablishSecureSessionDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertFetchMailboxC = FfiResult Function();
typedef IntrovertFetchMailboxDart = FfiResult Function();

typedef IntrovertStartMediaStreamC = FfiResult Function(Pointer<Utf8> peerId, Uint8 mediaType);
typedef IntrovertStartMediaStreamDart = FfiResult Function(Pointer<Utf8> peerId, int mediaType);

typedef IntrovertStorageGetContactsC = FfiResult Function();
typedef IntrovertStorageGetContactsDart = FfiResult Function();

typedef IntrovertDeleteContactC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertDeleteContactDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertDeleteChatC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertDeleteChatDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertClearContactsC = FfiResult Function();
typedef IntrovertClearContactsDart = FfiResult Function();

typedef IntrovertWormholeStartC = FfiResult Function();
typedef IntrovertWormholeStartDart = FfiResult Function();

typedef IntrovertWormholeJoinC = FfiResult Function(Pointer<Utf8> code);
typedef IntrovertWormholeJoinDart = FfiResult Function(Pointer<Utf8> code);

typedef IntrovertCloseWebRtcC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertCloseWebRtcDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertRenegotiateWebRtcC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertRenegotiateWebRtcDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertSetAnchorModeC = FfiResult Function(Bool enabled);
typedef IntrovertSetAnchorModeDart = FfiResult Function(bool enabled);

typedef IntrovertGetAnchorModeC = Int32 Function();
typedef IntrovertGetAnchorModeDart = int Function();

typedef IntrovertStorageGetProfileC = FfiResult Function();
typedef IntrovertStorageGetProfileDart = FfiResult Function();

typedef IntrovertStorageSetProfileC = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> avatar);
typedef IntrovertStorageSetProfileDart = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> avatar);

typedef IntrovertNetworkSendFileC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> filePath, Pointer<Utf8> groupId);
typedef IntrovertNetworkSendFileDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> filePath, Pointer<Utf8> groupId);

typedef IntrovertNetworkCancelFileTransferC = FfiResult Function(Pointer<Utf8> transferId);
typedef IntrovertNetworkCancelFileTransferDart = FfiResult Function(Pointer<Utf8> transferId);

typedef IntrovertNetworkForceRefreshC = FfiResult Function();
typedef IntrovertNetworkForceRefreshDart = FfiResult Function();

typedef IntrovertGroupCreateC = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> description, Pointer<Utf8> membersJson);
typedef IntrovertGroupCreateDart = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> description, Pointer<Utf8> membersJson);

typedef IntrovertGroupSendMessageC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> message);
typedef IntrovertGroupSendMessageDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> message);

typedef IntrovertGroupGetAllC = FfiResult Function();
typedef IntrovertGroupGetAllDart = FfiResult Function();

typedef IntrovertGroupGetMessagesC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupGetMessagesDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupAddMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupAddMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupRemoveMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupRemoveMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupUpdateRoleC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Int32 role);
typedef IntrovertGroupUpdateRoleDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, int role);

typedef IntrovertGroupPublishManifestC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> code);
typedef IntrovertGroupPublishManifestDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> code);

typedef IntrovertGroupJoinByCodeC = FfiResult Function(Pointer<Utf8> code);
typedef IntrovertGroupJoinByCodeDart = FfiResult Function(Pointer<Utf8> code);

typedef IntrovertGroupDeleteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupDeleteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupGetPendingInvitesC = FfiResult Function();
typedef IntrovertGroupGetPendingInvitesDart = FfiResult Function();

typedef IntrovertGroupAcceptInviteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupAcceptInviteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupDeclineInviteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupDeclineInviteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertNukeIdentityC = FfiResult Function(Pointer<Utf8> dbPath);
typedef IntrovertNukeIdentityDart = FfiResult Function(Pointer<Utf8> dbPath);

typedef IntrovertDriveAddFileC = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, Int64 size, Pointer<Utf8> localPath);
typedef IntrovertDriveAddFileDart = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, int size, Pointer<Utf8> localPath);

typedef IntrovertDriveGetAllC = FfiResult Function();
typedef IntrovertDriveGetAllDart = FfiResult Function();

typedef IntrovertDriveDeleteC = FfiResult Function(Pointer<Utf8> fileHash);
typedef IntrovertDriveDeleteDart = FfiResult Function(Pointer<Utf8> fileHash);

typedef IntrovertGetMeshCapacityC = Int64 Function();
typedef IntrovertGetMeshCapacityDart = int Function();

typedef IntrovertGetDiskSpaceC = Int32 Function(Pointer<Utf8> path, Pointer<Uint64> totalBytes, Pointer<Uint64> freeBytes);
typedef IntrovertGetDiskSpaceDart = int Function(Pointer<Utf8> path, Pointer<Uint64> totalBytes, Pointer<Uint64> freeBytes);

typedef IntrovertNetworkRegisterSeederC = FfiResult Function(Pointer<Utf8> transferId, Pointer<Utf8> filePath, Pointer<Utf8> fileHash, Int64 totalSize, Pointer<Utf8> groupId);
typedef IntrovertNetworkRegisterSeederDart = FfiResult Function(Pointer<Utf8> transferId, Pointer<Utf8> filePath, Pointer<Utf8> fileHash, int totalSize, Pointer<Utf8> groupId);

typedef IntrovertNetworkStartPullC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> transferId, Pointer<Utf8> filename, Pointer<Utf8> mimeType, Pointer<Utf8> fileHash, Int64 totalSize, Bool isRelayed, Pointer<Utf8> groupId);
typedef IntrovertNetworkStartPullDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> transferId, Pointer<Utf8> filename, Pointer<Utf8> mimeType, Pointer<Utf8> fileHash, int totalSize, bool isRelayed, Pointer<Utf8> groupId);

// --- Event Models ---

class NetworkEvent implements Finalizable {
  final int type;
  final Uint8List data;
  NetworkEvent(this.type, this.data);
}

class MediaFrameEvent implements Finalizable {
  final int codec;
  final int width;
  final int height;
  final Pointer<Uint8> payload;
  final int payloadLen;
  final Pointer<Uint8> basePtr;
  final int baseLen;

  MediaFrameEvent({
    required this.codec,
    required this.width,
    required this.height,
    required this.payload,
    required this.payloadLen,
    required this.basePtr,
    required this.baseLen,
  });
}

class FileTransferProgress {
  final String transferId;
  final String peerId;
  final String filename;
  final String mimeType;
  final double progress;
  final double speedBps;
  final bool isComplete;
  final bool isVerified;
  final bool isOutgoing;
  final bool isCancelled;
  final String? localPath;
  final int startTimeMs;

  FileTransferProgress({
    required this.transferId,
    required this.peerId,
    required this.filename,
    required this.mimeType,
    required this.progress,
    required this.speedBps,
    required this.isComplete,
    required this.isVerified,
    required this.isOutgoing,
    required this.isCancelled,
    this.localPath,
    required this.startTimeMs,
  });

  factory FileTransferProgress.fromJson(Map<String, dynamic> json) {
    return FileTransferProgress(
      transferId: json['transfer_id'],
      peerId: json['peer_id'],
      filename: json['filename'],
      mimeType: json['mime_type'] ?? 'application/octet-stream',
      progress: (json['progress'] as num).toDouble(),
      speedBps: (json['speed_bps'] as num?)?.toDouble() ?? 0.0,
      isComplete: json['is_complete'],
      isVerified: json['is_verified'] ?? false,
      isOutgoing: json['is_outgoing'],
      isCancelled: json['is_cancelled'] ?? false,
      localPath: json['local_path'],
      startTimeMs: json['start_time_ms'] ?? 0,
    );
  }
}

// --- Main Client Implementation ---

typedef IntrovertStorageUpdateMessageStatusC = FfiResult Function(Pointer<Utf8> msgId, Uint8 status);
typedef IntrovertStorageUpdateMessageStatusDart = FfiResult Function(Pointer<Utf8> msgId, int status);

typedef IntrovertNetworkSendAcknowledgementC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msgId, Uint8 status);
typedef IntrovertNetworkSendAcknowledgementDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msgId, int status);

typedef IntrovertStorageUpdateContactAliasC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> alias);
typedef IntrovertStorageUpdateContactAliasDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> alias);

class IntrovertClient {
  static final IntrovertClient _instance = IntrovertClient._internal();
  factory IntrovertClient() => _instance;

  late DynamicLibrary _dylib;
  NativeFinalizer? _binaryFinalizer;
  
  late IntrovertGenerateMnemonicDart _generateMnemonic;
  late IntrovertFreeStringDart _freeString;
  late IntrovertFreeBinaryDart _freeBinary;
  late IntrovertMnemonicToSeedDart _mnemonicToSeed;
  late IntrovertDeriveIdentifiersDart _deriveIdentifiers;
  late IntrovertEngineStartDart _engineStart;
  late IntrovertEngineStopDart _engineStop;
  late IntrovertGetPeerIdDart _getPeerId;
  late IntrovertNetworkStartDart _networkStart;
  late IntrovertEconomyStartMonitoringDart _economyStartMonitoring;
  late IntrovertNetworkSendMessageDart _networkSendMessage;
  late IntrovertNetworkInitiateWebRtcDart _networkInitiateWebRtc;
  late IntrovertAddAddressDart _addAddress;
  late IntrovertClaimRewardsAsyncDart _claimRewardsAsync;
  late IntrovertStoreMessageAsyncDart _storeMessageAsync;
  late IntrovertStorageGetMessagesDart _getMessages;
  late IntrovertEstablishSecureSessionDart _establishSecureSession;
  late IntrovertFetchMailboxDart _fetchMailbox;
  late IntrovertStartMediaStreamDart _startMediaStream;
  late IntrovertStorageGetContactsDart _getContacts;
  late IntrovertDeleteContactDart _deleteContact;
  late IntrovertDeleteChatDart _deleteChat;
  late IntrovertClearContactsDart _clearContacts;
  late IntrovertWormholeStartDart _wormholeStart;
  late IntrovertWormholeJoinDart _wormholeJoin;
  late IntrovertCloseWebRtcDart _closeWebRtc;
  late IntrovertRenegotiateWebRtcDart _renegotiateWebRtc;
  late IntrovertSetAnchorModeDart _setAnchorMode;
  late IntrovertGetAnchorModeDart _getAnchorMode;
  late IntrovertStorageGetProfileDart _getProfile;
  late IntrovertStorageSetProfileDart _setProfile;
  late IntrovertNetworkSendFileDart _sendFile;
  late IntrovertNetworkCancelFileTransferDart _cancelFileTransfer;
  late IntrovertNetworkForceRefreshDart _forceNetworkRefresh;
  late IntrovertGroupCreateDart _groupCreate;
  late IntrovertGroupSendMessageDart _groupSendMessage;
  late IntrovertGroupGetAllDart _groupGetAll;
  late IntrovertGroupGetMessagesDart _groupGetMessages;
  late IntrovertGroupAddMemberDart _groupAddMember;
  late IntrovertGroupRemoveMemberDart _groupRemoveMember;
  late IntrovertGroupUpdateRoleDart _groupUpdateRole;
  late IntrovertGroupPublishManifestDart _groupPublishManifest;
  late IntrovertGroupJoinByCodeDart _groupJoinByCode;
  late IntrovertGroupDeleteDart _groupDelete;
  late IntrovertGroupGetPendingInvitesDart _groupGetPendingInvites;
  late IntrovertGroupAcceptInviteDart _groupAcceptInvite;
  late IntrovertGroupDeclineInviteDart _groupDeclineInvite;
  late IntrovertStorageUpdateMessageStatusDart _updateMessageStatus;
  late IntrovertNetworkSendAcknowledgementDart _sendAcknowledgement;
  late IntrovertStorageUpdateContactAliasDart _updateContactAlias;
  late IntrovertNukeIdentityDart _nukeIdentity;
  late IntrovertDriveAddFileDart _driveAddFile;
  late IntrovertDriveGetAllDart _driveGetAll;
  late IntrovertDriveDeleteDart _driveDelete;
  late IntrovertGetMeshCapacityDart _getMeshCapacity;
  late IntrovertGetDiskSpaceDart _getDiskSpace;
  late IntrovertNetworkRegisterSeederDart _registerSeeder;
  late IntrovertNetworkStartPullDart _startPull;

  NativeCallable<NativeNetworkCallback>? _networkCallable;
  NativeCallable<NativeNetworkCallback>? _economyCallable;

  int? _lastLocalStatus;
  int? get localStatus => _lastLocalStatus;

  final StreamController<NetworkEvent> _networkStreamController = StreamController<NetworkEvent>.broadcast();
  Stream<NetworkEvent> get networkStream => _networkStreamController.stream;

  final StreamController<MediaFrameEvent> _mediaStreamController = StreamController<MediaFrameEvent>.broadcast();
  Stream<MediaFrameEvent> get mediaStream => _mediaStreamController.stream;

  final StreamController<FileTransferProgress> _transferStreamController = StreamController<FileTransferProgress>.broadcast();
  Stream<FileTransferProgress> get transferStream => _transferStreamController.stream;

  final StreamController<Map<String, dynamic>> _economyStreamController = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get economyStream => _economyStreamController.stream;

  IntrovertClient._internal() {
    _loadLibrary();
    _bindFunctions();
    _initializeFinalizer();
  }

  void _initializeFinalizer() {
    try {
      final freeFunc = _dylib.lookup<NativeFunction<Void Function(Pointer<Void>)>>('introvert_free_binary_finalizer');
      _binaryFinalizer = NativeFinalizer(freeFunc.cast());
      debugPrint('✅ Native finalizer initialized.');
    } catch (e) {
      debugPrint('ℹ️ Native finalizer NOT available: $e');
      _binaryFinalizer = null;
    }
  }

  void _loadLibrary() {
    if (Platform.isAndroid || Platform.isLinux) {
      try {
        _dylib = DynamicLibrary.open('libintrovert.so');
        debugPrint('✅ Loaded libintrovert.so');
      } catch (e) {
        debugPrint('❌ Failed to load libintrovert.so: $e');
        rethrow;
      }
    } else if (Platform.isMacOS) {
      final List<String> possiblePaths = [
        '${Directory.current.path}/libintrovert.dylib',
        '${Directory.current.path}/macos/Flutter/ephemeral/libintrovert.dylib',
        'libintrovert.dylib',
      ];

      debugPrint('ℹ️ macOS: Searching for libintrovert.dylib...');
      for (final path in possiblePaths) {
        try {
          if (path.startsWith('/') && !File(path).existsSync()) continue;
          _dylib = DynamicLibrary.open(path);
          debugPrint('✅ Loaded native library: $path');
          return;
        } catch (e) {
          // Continue
        }
      }

      try {
        _dylib = DynamicLibrary.process();
        debugPrint('ℹ️ Falling back to process-level lookup');
      } catch (e) {
        throw UnsupportedError('Could not load libintrovert.dylib: $e');
      }
    } else if (Platform.isIOS) {
      _dylib = DynamicLibrary.process();
    } else {
      throw UnsupportedError('Unsupported platform.');
    }
  }

  void _bindFunctions() {
    T safeLookup<T>(String name, T Function() lookup, T fallback) {
      try {
        final result = lookup();
        debugPrint('✅ FFI Bound: $name');
        return result;
      } catch (e) {
        debugPrint('⚠️ FFI Bind FAILED: $name (Using placeholder)');
        return fallback;
      }
    }

    try {
      _generateMnemonic = safeLookup('generate_mnemonic', () => _dylib.lookupFunction<IntrovertGenerateMnemonicC, IntrovertGenerateMnemonicDart>('introvert_generate_mnemonic'), () => nullptr);
      _freeString = safeLookup('free_string', () => _dylib.lookupFunction<IntrovertFreeStringC, IntrovertFreeStringDart>('introvert_free_string'), (ptr) {});
      _freeBinary = safeLookup('free_binary', () => _dylib.lookupFunction<IntrovertFreeBinaryC, IntrovertFreeBinaryDart>('introvert_free_binary'), (ptr, len) {});
      _mnemonicToSeed = safeLookup('mnemonic_to_seed', () => _dylib.lookupFunction<IntrovertMnemonicToSeedC, IntrovertMnemonicToSeedDart>('introvert_mnemonic_to_seed'), (ptr) => FfiResult.dummy);
      _deriveIdentifiers = safeLookup('derive_identifiers', () => _dylib.lookupFunction<IntrovertDeriveIdentifiersC, IntrovertDeriveIdentifiersDart>('introvert_derive_identifiers'), (s) => FfiResult.dummy);
      _engineStart = safeLookup('engine_start', () => _dylib.lookupFunction<IntrovertEngineStartC, IntrovertEngineStartDart>('introvert_engine_start'), (s, p) => FfiResult.dummy);
      _engineStop = safeLookup('engine_stop', () => _dylib.lookupFunction<IntrovertEngineStopC, IntrovertEngineStopDart>('introvert_engine_stop'), () => FfiResult.dummy);
      _getPeerId = safeLookup('get_peer_id', () => _dylib.lookupFunction<IntrovertGetPeerIdC, IntrovertGetPeerIdDart>('introvert_get_peer_id'), () => nullptr);
      _networkStart = safeLookup('network_start', () => _dylib.lookupFunction<IntrovertNetworkStartC, IntrovertNetworkStartDart>('introvert_network_start_production'), (cb, p, r, m, l) => FfiResult.dummy);
      _economyStartMonitoring = safeLookup('economy_monitor', () => _dylib.lookupFunction<IntrovertEconomyStartMonitoringC, IntrovertEconomyStartMonitoringDart>('introvert_economy_start_monitoring'), (cb) => FfiResult.dummy);
      _networkSendMessage = safeLookup('send_message', () => _dylib.lookupFunction<IntrovertNetworkSendMessageC, IntrovertNetworkSendMessageDart>('introvert_network_send_message'), (p, m, cb) => FfiResult.dummy);
      _networkInitiateWebRtc = safeLookup('init_webrtc', () => _dylib.lookupFunction<IntrovertNetworkInitiateWebRtcC, IntrovertNetworkInitiateWebRtcDart>('introvert_network_initiate_webrtc'), (p, cb) => FfiResult.dummy);
      _addAddress = safeLookup('add_address', () => _dylib.lookupFunction<IntrovertAddAddressC, IntrovertAddAddressDart>('introvert_network_add_address'), (p, a) => FfiResult.dummy);
      _claimRewardsAsync = safeLookup('claim_rewards', () => _dylib.lookupFunction<IntrovertClaimRewardsAsyncC, IntrovertClaimRewardsAsyncDart>('introvert_claim_rewards_async'), (cb) => FfiResult.dummy);
      _storeMessageAsync = safeLookup('store_msg_async', () => _dylib.lookupFunction<IntrovertStoreMessageAsyncC, IntrovertStoreMessageAsyncDart>('introvert_store_message_async'), (p, m, me, cb) => FfiResult.dummy);
      _getMessages = safeLookup('get_messages', () => _dylib.lookupFunction<IntrovertStorageGetMessagesC, IntrovertStorageGetMessagesDart>('introvert_storage_get_messages'), (p) => FfiResult.dummy);
      _establishSecureSession = safeLookup('secure_session', () => _dylib.lookupFunction<IntrovertEstablishSecureSessionC, IntrovertEstablishSecureSessionDart>('introvert_network_establish_secure_session'), (p) => FfiResult.dummy);
      _fetchMailbox = safeLookup('fetch_mailbox', () => _dylib.lookupFunction<IntrovertFetchMailboxC, IntrovertFetchMailboxDart>('introvert_network_fetch_mailbox'), () => FfiResult.dummy);
      _startMediaStream = safeLookup('media_stream', () => _dylib.lookupFunction<IntrovertStartMediaStreamC, IntrovertStartMediaStreamDart>('introvert_network_start_media_stream'), (p, t) => FfiResult.dummy);
      _getContacts = safeLookup('get_contacts', () => _dylib.lookupFunction<IntrovertStorageGetContactsC, IntrovertStorageGetContactsDart>('introvert_storage_get_contacts'), () => FfiResult.dummy);
      _deleteContact = safeLookup('delete_contact', () => _dylib.lookupFunction<IntrovertDeleteContactC, IntrovertDeleteContactDart>('introvert_storage_delete_contact'), (p) => FfiResult.dummy);
      _deleteChat = safeLookup('delete_chat', () => _dylib.lookupFunction<IntrovertDeleteChatC, IntrovertDeleteChatDart>('introvert_storage_delete_chat'), (p) => FfiResult.dummy);
      _clearContacts = safeLookup('clear_contacts', () => _dylib.lookupFunction<IntrovertClearContactsC, IntrovertClearContactsDart>('introvert_storage_clear_contacts'), () => FfiResult.dummy);
      _wormholeStart = safeLookup('wormhole_start', () => _dylib.lookupFunction<IntrovertWormholeStartC, IntrovertWormholeStartDart>('introvert_wormhole_start'), () => FfiResult.dummy);
      _wormholeJoin = safeLookup('wormhole_join', () => _dylib.lookupFunction<IntrovertWormholeJoinC, IntrovertWormholeJoinDart>('introvert_wormhole_join'), (c) => FfiResult.dummy);
      _closeWebRtc = safeLookup('close_webrtc', () => _dylib.lookupFunction<IntrovertCloseWebRtcC, IntrovertCloseWebRtcDart>('introvert_webrtc_close_connection'), (p) => FfiResult.dummy);
      _renegotiateWebRtc = safeLookup('renegotiate_webrtc', () => _dylib.lookupFunction<IntrovertRenegotiateWebRtcC, IntrovertRenegotiateWebRtcDart>('introvert_webrtc_renegotiate'), (p) => FfiResult.dummy);
      _setAnchorMode = safeLookup('set_anchor', () => _dylib.lookupFunction<IntrovertSetAnchorModeC, IntrovertSetAnchorModeDart>('introvert_network_set_anchor_mode'), (e) => FfiResult.dummy);
      _getAnchorMode = safeLookup('get_anchor_mode', () => _dylib.lookupFunction<IntrovertGetAnchorModeC, IntrovertGetAnchorModeDart>('introvert_network_get_anchor_mode'), () => 0);
      _getProfile = safeLookup('get_profile', () => _dylib.lookupFunction<IntrovertStorageGetProfileC, IntrovertStorageGetProfileDart>('introvert_storage_get_profile'), () => FfiResult.dummy);
      _setProfile = safeLookup('set_profile', () => _dylib.lookupFunction<IntrovertStorageSetProfileC, IntrovertStorageSetProfileDart>('introvert_storage_set_profile'), (n, a) => FfiResult.dummy);
      _sendFile = safeLookup('send_file', () => _dylib.lookupFunction<IntrovertNetworkSendFileC, IntrovertNetworkSendFileDart>('introvert_network_send_file'), (p, f, g) => FfiResult.dummy);
      _cancelFileTransfer = safeLookup('cancel_file', () => _dylib.lookupFunction<IntrovertNetworkCancelFileTransferC, IntrovertNetworkCancelFileTransferDart>('introvert_network_cancel_file_transfer'), (id) => FfiResult.dummy);
      _forceNetworkRefresh = safeLookup('force_refresh', () => _dylib.lookupFunction<IntrovertNetworkForceRefreshC, IntrovertNetworkForceRefreshDart>('introvert_network_force_refresh'), () => FfiResult.dummy);
      _groupCreate = safeLookup('group_create', () => _dylib.lookupFunction<IntrovertGroupCreateC, IntrovertGroupCreateDart>('introvert_group_create'), (n, d, m) => FfiResult.dummy);
      _groupSendMessage = safeLookup('group_send', () => _dylib.lookupFunction<IntrovertGroupSendMessageC, IntrovertGroupSendMessageDart>('introvert_group_send_message'), (g, m) => FfiResult.dummy);
      _groupGetAll = safeLookup('group_get_all', () => _dylib.lookupFunction<IntrovertGroupGetAllC, IntrovertGroupGetAllDart>('introvert_group_get_all'), () => FfiResult.dummy);
      _groupGetMessages = safeLookup('group_get_msgs', () => _dylib.lookupFunction<IntrovertGroupGetMessagesC, IntrovertGroupGetMessagesDart>('introvert_group_get_messages'), (g) => FfiResult.dummy);
      _groupAddMember = safeLookup('group_add_member', () => _dylib.lookupFunction<IntrovertGroupAddMemberC, IntrovertGroupAddMemberDart>('introvert_group_add_member'), (g, p) => FfiResult.dummy);
      _groupRemoveMember = safeLookup('group_remove_member', () => _dylib.lookupFunction<IntrovertGroupRemoveMemberC, IntrovertGroupRemoveMemberDart>('introvert_group_remove_member'), (g, p) => FfiResult.dummy);
      _groupUpdateRole = safeLookup('group_update_role', () => _dylib.lookupFunction<IntrovertGroupUpdateRoleC, IntrovertGroupUpdateRoleDart>('introvert_group_update_role'), (g, p, r) => FfiResult.dummy);
      _groupPublishManifest = safeLookup('group_publish', () => _dylib.lookupFunction<IntrovertGroupPublishManifestC, IntrovertGroupPublishManifestDart>('introvert_group_publish_manifest'), (g, c) => FfiResult.dummy);
      _groupJoinByCode = safeLookup('group_join_code', () => _dylib.lookupFunction<IntrovertGroupJoinByCodeC, IntrovertGroupJoinByCodeDart>('introvert_group_join_by_code'), (c) => FfiResult.dummy);
      _groupDelete = safeLookup('group_delete', () => _dylib.lookupFunction<IntrovertGroupDeleteC, IntrovertGroupDeleteDart>('introvert_group_delete'), (g) => FfiResult.dummy);
      _groupGetPendingInvites = safeLookup('group_get_pending', () => _dylib.lookupFunction<IntrovertGroupGetPendingInvitesC, IntrovertGroupGetPendingInvitesDart>('introvert_group_get_pending_invites'), () => FfiResult.dummy);
      _groupAcceptInvite = safeLookup('group_accept_invite', () => _dylib.lookupFunction<IntrovertGroupAcceptInviteC, IntrovertGroupAcceptInviteDart>('introvert_group_accept_invite'), (g) => FfiResult.dummy);
      _groupDeclineInvite = safeLookup('group_decline_invite', () => _dylib.lookupFunction<IntrovertGroupDeclineInviteC, IntrovertGroupDeclineInviteDart>('introvert_group_decline_invite'), (g) => FfiResult.dummy);
      _updateMessageStatus = safeLookup('update_msg_status', () => _dylib.lookupFunction<IntrovertStorageUpdateMessageStatusC, IntrovertStorageUpdateMessageStatusDart>('introvert_storage_update_message_status'), (m, s) => FfiResult.dummy);
      _sendAcknowledgement = safeLookup('send_ack', () => _dylib.lookupFunction<IntrovertNetworkSendAcknowledgementC, IntrovertNetworkSendAcknowledgementDart>('introvert_network_send_acknowledgement'), (p, m, s) => FfiResult.dummy);
      _updateContactAlias = safeLookup('update_contact_alias', () => _dylib.lookupFunction<IntrovertStorageUpdateContactAliasC, IntrovertStorageUpdateContactAliasDart>('introvert_storage_update_contact_alias'), (p, a) => FfiResult.dummy);
      _nukeIdentity = safeLookup('nuke_identity', () => _dylib.lookupFunction<IntrovertNukeIdentityC, IntrovertNukeIdentityDart>('introvert_nuke_identity'), (db) => FfiResult.dummy);
      _driveAddFile = safeLookup('drive_add', () => _dylib.lookupFunction<IntrovertDriveAddFileC, IntrovertDriveAddFileDart>('introvert_drive_add_file'), (n, h, m, s, p) => FfiResult.dummy);
      _driveGetAll = safeLookup('drive_get_all', () => _dylib.lookupFunction<IntrovertDriveGetAllC, IntrovertDriveGetAllDart>('introvert_drive_get_all'), () => FfiResult.dummy);
      _driveDelete = safeLookup('drive_delete', () => _dylib.lookupFunction<IntrovertDriveDeleteC, IntrovertDriveDeleteDart>('introvert_drive_delete'), (h) => FfiResult.dummy);
      _getMeshCapacity = safeLookup('mesh_capacity', () => _dylib.lookupFunction<IntrovertGetMeshCapacityC, IntrovertGetMeshCapacityDart>('introvert_get_mesh_capacity'), () => 0);
      _getDiskSpace = safeLookup('get_disk_space', () => _dylib.lookupFunction<IntrovertGetDiskSpaceC, IntrovertGetDiskSpaceDart>('introvert_get_disk_space'), (path, total, free) => -1);
      _registerSeeder = safeLookup('register_seeder', () => _dylib.lookupFunction<IntrovertNetworkRegisterSeederC, IntrovertNetworkRegisterSeederDart>('introvert_network_register_seeder'), (t, p, h, s, g) => FfiResult.dummy);
      _startPull = safeLookup('start_pull', () => _dylib.lookupFunction<IntrovertNetworkStartPullC, IntrovertNetworkStartPullDart>('introvert_network_start_pull'), (p, t, n, m, h, s, r, g) => FfiResult.dummy);
      debugPrint('✅ All native functions bound successfully.');
    } catch (e) {
      debugPrint('❌ Error binding native functions: $e');
    }
  }

  void forceNetworkRefresh() {
    _forceNetworkRefresh();
  }

  void createGroup(String name, String description, List<String> members) {
    using((Arena arena) {
      _groupCreate(
        name.toNativeUtf8(allocator: arena),
        description.toNativeUtf8(allocator: arena),
        json.encode(members).toNativeUtf8(allocator: arena),
      );
    });
  }

  void deleteGroup(String groupId) {
    using((Arena arena) {
      _groupDelete(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  List<dynamic> getPendingGroupInvites() {
    final res = _groupGetPendingInvites();
    if (res.code != 0) return [];
    try {
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void acceptGroupInvite(String groupId) {
    using((Arena arena) {
      _groupAcceptInvite(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void declineGroupInvite(String groupId) {
    using((Arena arena) {
      _groupDeclineInvite(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void registerSeeder(String transferId, String filePath, String fileHash, int totalSize, [String? groupId]) {
    using((Arena arena) {
      _registerSeeder(
        transferId.toNativeUtf8(allocator: arena),
        filePath.toNativeUtf8(allocator: arena),
        fileHash.toNativeUtf8(allocator: arena),
        totalSize,
        (groupId ?? "").toNativeUtf8(allocator: arena),
      );
    });
  }

  void startPull(String peerId, String transferId, String filename, String mimeType, String fileHash, int totalSize, bool isRelayed, [String? groupId]) {
    using((Arena arena) {
      _startPull(
        peerId.toNativeUtf8(allocator: arena),
        transferId.toNativeUtf8(allocator: arena),
        filename.toNativeUtf8(allocator: arena),
        mimeType.toNativeUtf8(allocator: arena),
        fileHash.toNativeUtf8(allocator: arena),
        totalSize,
        isRelayed,
        groupId != null ? groupId.toNativeUtf8(allocator: arena) : nullptr,
      );
    });
  }

  void sendGroupMessage(String groupId, String message) {
    using((Arena arena) {
      _groupSendMessage(
        groupId.toNativeUtf8(allocator: arena),
        message.toNativeUtf8(allocator: arena),
      );
    });
  }

  List<dynamic> getAllGroups() {
    final res = _groupGetAll();
    if (res.code != 0) return [];
    try {
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> getGroupMessages(String groupId) {
    late FfiResult res;
    using((Arena arena) => res = _groupGetMessages(groupId.toNativeUtf8(allocator: arena)));
    if (res.code != 0) return [];
    try {
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void addGroupMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupAddMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void removeGroupMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupRemoveMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void updateGroupRole(String groupId, String peerId, int role) {
    using((Arena arena) {
      _groupUpdateRole(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
        role,
      );
    });
  }

  void publishGroupManifest(String groupId, String code) {
    using((Arena arena) {
      _groupPublishManifest(
        groupId.toNativeUtf8(allocator: arena),
        code.toNativeUtf8(allocator: arena),
      );
    });
  }

  void joinMeshByCode(String code) {
    using((Arena arena) {
      _groupJoinByCode(
        code.toNativeUtf8(allocator: arena),
      );
    });
  }

  void updateMessageStatus(String msgId, int status) {
    using((Arena arena) => _updateMessageStatus(msgId.toNativeUtf8(allocator: arena), status));
  }

  void sendAcknowledgement(String peerId, String msgId, int status) {
    using((Arena arena) => _sendAcknowledgement(
      peerId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      status
    ));
  }

  void updateContactAlias(String peerId, String alias) {
    using((Arena arena) => _handleFfiResult(_updateContactAlias(
      peerId.toNativeUtf8(allocator: arena),
      alias.toNativeUtf8(allocator: arena),
    ), context: "Update Contact Alias"));
  }

  void nukeIdentity(String dbPath) {
    using((Arena arena) => _handleFfiResult(_nukeIdentity(dbPath.toNativeUtf8(allocator: arena)), context: "Nuke Identity"));
  }

  void driveAddFile(String name, String hash, String mime, int size, String path) {
    using((Arena arena) => _handleFfiResult(_driveAddFile(
      name.toNativeUtf8(allocator: arena),
      hash.toNativeUtf8(allocator: arena),
      mime.toNativeUtf8(allocator: arena),
      size,
      path.toNativeUtf8(allocator: arena),
    ), context: "Drive Add File"));
  }

  List<dynamic> driveGetAll() {
    final res = _driveGetAll();
    if (res.code != 0) return [];
    try {
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void driveDelete(String hash) {
    using((Arena arena) => _handleFfiResult(_driveDelete(hash.toNativeUtf8(allocator: arena)), context: "Drive Delete"));
  }

  Map<String, int> getDiskSpace(String path) {
    return using((Arena arena) {
      final pathPtr = path.toNativeUtf8(allocator: arena);
      final totalBytesPtr = arena<Uint64>();
      final freeBytesPtr = arena<Uint64>();
      
      final res = _getDiskSpace(pathPtr, totalBytesPtr, freeBytesPtr);
      if (res == 0) {
        return {
          'total': totalBytesPtr.value,
          'free': freeBytesPtr.value,
        };
      } else {
        return {
          'total': 0,
          'free': 0,
        };
      }
    });
  }

  int getMeshCapacity() => _getMeshCapacity();

  void startNetwork({int port = 0, bool relay = false, int maxConn = 1024, int liveness = 600}) {
    if (_networkCallable != null) return;
    debugPrint('📡 Initializing Network Plane (Port: $port, Relay: $relay)...');
    _networkCallable = NativeCallable<NativeNetworkCallback>.listener((int eventType, Pointer<Uint8> dataPtr, int dataLen) {
      if (dataPtr.address == 0) return;
      
      final Pointer<Uint8> castedPtr = dataPtr.cast<Uint8>();
      if (eventType == 5) {
        final header = castedPtr.cast<MediaFrameHeader>().ref;
        final headerSize = sizeOf<MediaFrameHeader>();
        final event = MediaFrameEvent(
          codec: header.codec,
          width: header.width,
          height: header.height,
          payload: Pointer<Uint8>.fromAddress(castedPtr.address + headerSize),
          payloadLen: dataLen - headerSize,
          basePtr: castedPtr,
          baseLen: dataLen,
        );
        _binaryFinalizer?.attach(event, castedPtr.cast<Void>(), externalSize: dataLen);
        _mediaStreamController.add(event);
      } else if (eventType == 7 || eventType == 11 || eventType == 12 || eventType == 13) {
        final data = castedPtr.asTypedList(dataLen);
        if (eventType == 12) {
          try {
            final jsonStr = utf8.decode(data);
            final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
            _transferStreamController.add(progress);
          } catch (e) {
            debugPrint("❌ Error decoding file progress: $e");
          } finally {
            _freeBinary(dataPtr, dataLen);
          }
        } else if (eventType == 13) {
           // event 13: Acknowledgement [StatusByte, msg_id_bytes]
           _networkStreamController.add(NetworkEvent(eventType, Uint8List.fromList(data)));
           _freeBinary(dataPtr, dataLen);
        } else {
           // event 7: Secure Handshake Complete
           // event 11: Anchor status or other unhandled
           _networkStreamController.add(NetworkEvent(eventType, Uint8List.fromList(data)));
           _freeBinary(dataPtr, dataLen);
        }
      } else {
        if (eventType == 10 && dataLen > 0) {
          _lastLocalStatus = castedPtr.asTypedList(dataLen)[0];
        }
        final data = castedPtr.asTypedList(dataLen);
        if (eventType == 99) {
          debugPrint('🦀 Rust Debug: ${utf8.decode(data)}');
          _freeBinary(dataPtr, dataLen);
          return;
        }
        final eventData = Uint8List.fromList(data);
        final event = NetworkEvent(eventType, eventData);
        _binaryFinalizer?.attach(event, castedPtr.cast<Void>(), externalSize: dataLen);
        _networkStreamController.add(event);
      }
    });
    _handleFfiResult(_networkStart(_networkCallable!.nativeFunction, port, relay, maxConn, liveness), context: "Network Start");
  }

  void startEconomyMonitoring(void Function(Map<String, dynamic> stats) onUpdate) {
    _economyCallable?.close();
    _economyCallable = NativeCallable<NativeNetworkCallback>.listener((int eventType, Pointer<Uint8> dataPtr, int dataLen) {
      if (dataPtr.address == 0) return;
      try {
        if (eventType == 9) {
          final data = dataPtr.cast<Uint8>().asTypedList(dataLen);
          final stats = json.decode(utf8.decode(data)) as Map<String, dynamic>;
          if (!stats.containsKey('sol_balance')) {
            stats['sol_balance'] = stats['intr_balance'] ?? 0;
          }
          onUpdate(stats);
          _economyStreamController.add(stats);
        }
      } finally {
        _freeBinary(dataPtr, dataLen);
      }
    });
    _handleFfiResult(_economyStartMonitoring(_economyCallable!.nativeFunction), context: "Economy Monitoring");
  }

  void startWormholeInvite() => _handleFfiResult(_wormholeStart(), context: "Wormhole Start");

  void joinWormholeInvite(String code) {
    using((Arena arena) => _handleFfiResult(_wormholeJoin(code.toNativeUtf8(allocator: arena)), context: "Wormhole Join"));
  }

  String generateMnemonic() {
    final ptr = _generateMnemonic();
    if (ptr.address == 0) throw Exception("Mnemonic generation failed");
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  Uint8List mnemonicToSeed(String phrase) {
    return using((Arena arena) {
      final res = _mnemonicToSeed(phrase.toNativeUtf8(allocator: arena));
      if (res.code != 0) throw Exception("Seed derivation failed (${res.code})");
      final data = Uint8List.fromList(res.data.asTypedList(res.len));
      _freeBinary(res.data, res.len);
      return data;
    });
  }

  Map<String, String> deriveIdentifiers(Uint8List seed) {
    return using((Arena arena) {
      final seedPtr = arena<Uint8>(32);
      seedPtr.asTypedList(32).setAll(0, seed);
      final res = _deriveIdentifiers(seedPtr);
      if (res.code != 0) throw Exception("Identifiers derivation failed (${res.code})");
      final jsonStr = utf8.decode(res.data.asTypedList(res.len));
      _freeBinary(res.data, res.len);
      final decoded = json.decode(jsonStr) as Map<String, dynamic>;
      return {
        'peer_id': decoded['peer_id']?.toString() ?? '',
        'solana_address': decoded['solana_address']?.toString() ?? '',
      };
    });
  }

  void startEngine(Uint8List seed, String dbPath) {
    using((Arena arena) {
      final seedPtr = arena<Uint8>(32);
      for (var i = 0; i < 32; i++) {
        seedPtr[i] = seed[i];
      }
      _handleFfiResult(_engineStart(seedPtr, dbPath.toNativeUtf8(allocator: arena)), context: "Engine Start");
    });
  }

  void stopEngine() => _handleFfiResult(_engineStop(), context: "Engine Stop");

  String? get localPeerId => getPeerId();

  String? getPeerId() {
    final ptr = _getPeerId();
    if (ptr.address == 0) return null;
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  List<dynamic> getContacts() {
    final res = _getContacts();
    if (res.code != 0) return [];
    try { return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>; } finally { _freeBinary(res.data, res.len); }
  }

  Future<void> deleteContact(String id) async => using((Arena arena) => _handleFfiResult(_deleteContact(id.toNativeUtf8(allocator: arena)), context: "Delete Contact"));
  Future<void> deleteChat(String id) async => using((Arena arena) => _handleFfiResult(_deleteChat(id.toNativeUtf8(allocator: arena)), context: "Delete Chat"));
  Future<void> clearAllContacts() async => _handleFfiResult(_clearContacts(), context: "Clear Contacts");

  Future<String> sendMessage(String id, String msg) async {
    final comp = Completer<String>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) {
      if (res.code == 0) {
        if (res.len > 0) {
          final data = res.data.cast<Uint8>().asTypedList(res.len);
          final msgId = utf8.decode(data);
          comp.complete(msgId);
        } else {
          comp.complete("");
        }
      } else {
        comp.completeError(Exception("Send failed"));
      }
      if (res.len > 0) _freeBinary(res.data, res.len);
    });
    using((Arena arena) => _networkSendMessage(id.toNativeUtf8(allocator: arena), msg.toNativeUtf8(allocator: arena), cb.nativeFunction));
    final msgId = await comp.future;
    cb.close();
    return msgId;
  }

  void addAddress(String id, String address) => using((Arena arena) => _handleFfiResult(_addAddress(id.toNativeUtf8(allocator: arena), address.toNativeUtf8(allocator: arena)), context: "Add Address"));

  Future<void> storeMessage(String id, String msg, {bool isMe = false}) async {
    final comp = Completer<void>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) {
      if (res.code == 0) {
        comp.complete();
      } else {
        comp.completeError(Exception("Store failed"));
      }
      if (res.len > 0) _freeBinary(res.data, res.len);
    });
    using((Arena arena) => _storeMessageAsync(id.toNativeUtf8(allocator: arena), msg.toNativeUtf8(allocator: arena), isMe, cb.nativeFunction));
    await comp.future; cb.close();
  }

  List<dynamic> getMessages(String peerId) {
    late FfiResult result;
    using((Arena arena) => result = _getMessages(peerId.toNativeUtf8(allocator: arena)));
    
    try {
      if (result.code == 0 && result.len > 0) {
        final data = result.data.cast<Uint8>().asTypedList(result.len);
        final jsonStr = utf8.decode(data);
        return json.decode(jsonStr);
      }
      return [];
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  void establishSecureSession(String id) => using((Arena arena) => _handleFfiResult(_establishSecureSession(id.toNativeUtf8(allocator: arena)), context: "Secure Session"));

  Future<void> initiateWebRtc(String peerId) async {
    final completer = Completer<void>();
    final callback = NativeCallable<NativeFfiCallback>.listener((FfiResult result) {
      if (result.code == 0) {
        completer.complete();
      } else {
        completer.completeError(Exception("WebRTC error (${result.code})"));
      }
      if (result.len > 0) _freeBinary(result.data, result.len);
    });
    using((Arena arena) => _networkInitiateWebRtc(peerId.toNativeUtf8(allocator: arena), callback.nativeFunction));
    await completer.future; callback.close();
  }

  void closeWebRtc(String peerId) => using((Arena arena) => _handleFfiResult(_closeWebRtc(peerId.toNativeUtf8(allocator: arena)), context: "Close WebRTC"));
  void renegotiateWebRtc(String peerId) => using((Arena arena) => _handleFfiResult(_renegotiateWebRtc(peerId.toNativeUtf8(allocator: arena)), context: "Renegotiate WebRTC"));

  void fetchMailbox() => _handleFfiResult(_fetchMailbox(), context: "Fetch Mailbox");
  void startMediaStream(String id, int type) => using((Arena arena) => _handleFfiResult(_startMediaStream(id.toNativeUtf8(allocator: arena), type), context: "Media Stream"));

  void setAnchorMode(bool enabled) => _handleFfiResult(_setAnchorMode(enabled), context: "Set Anchor Mode");
  bool isAnchorModeEnabled() => _getAnchorMode() == 1;

  Map<String, dynamic> getProfile() {
    final res = _getProfile();
    if (res.code != 0) return {};
    try { return json.decode(utf8.decode(res.data.asTypedList(res.len))) as Map<String, dynamic>; } finally { _freeBinary(res.data, res.len); }
  }

  void setProfile(String? name, String? avatar) {
    using((Arena arena) {
      _handleFfiResult(
        _setProfile(
          name?.toNativeUtf8(allocator: arena) ?? nullptr,
          avatar?.toNativeUtf8(allocator: arena) ?? nullptr,
        ),
        context: "Set Profile",
      );
    });
  }

  void sendFile(String peerId, String filePath, [String? groupId]) {
    using((Arena arena) {
      _handleFfiResult(
        _sendFile(
          peerId.toNativeUtf8(allocator: arena),
          filePath.toNativeUtf8(allocator: arena),
          (groupId ?? "").toNativeUtf8(allocator: arena),
        ),
        context: "Send File",
      );
    });
  }

  void cancelFileTransfer(String transferId) {
    using((Arena arena) {
      _handleFfiResult(
        _cancelFileTransfer(
          transferId.toNativeUtf8(allocator: arena),
        ),
        context: "Cancel File Transfer",
      );
    });
  }

  Future<String> claimRewards() async {
    final comp = Completer<String>();
    final cb = NativeCallable<NativeRewardCallback>.listener((int status, Pointer<Utf8> sigPtr) {
      if (status == 0) {
        final sig = sigPtr.toDartString();
        _freeString(sigPtr); 
        comp.complete(sig);
      } else {
        final err = sigPtr.toDartString();
        _freeString(sigPtr); 
        comp.completeError(Exception("Claim error: $err"));
      }
    });
    _claimRewardsAsync(cb.nativeFunction);
    final sig = await comp.future; cb.close();
    return sig;
  }

  void freeBinary(Pointer<Uint8> ptr, int len) => _freeBinary(ptr, len);

  void _handleFfiResult(FfiResult result, {String context = "Rust Core"}) {
    if (result.code != 0) {
      String msg = "Unknown error";
      if (result.data.address != 0) {
        msg = utf8.decode(result.data.asTypedList(result.len));
        _freeBinary(result.data, result.len);
      }
      debugPrint('❌ $context Error (${result.code}): $msg');
      throw Exception('$context Error (${result.code}): $msg');
    } else {
      debugPrint('✅ $context: Success');
    }
  }
}
