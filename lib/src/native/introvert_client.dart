import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';
import 'package:ffi/ffi.dart';

// --- Native C Signatures & Dart Mapping ---

final class FfiResult extends Struct {
  @Int32()
  external int code;
  external Pointer<Uint8> data;
  @Size()
  external int len;

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

typedef IntrovertEngineStartC = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);
typedef IntrovertEngineStartDart = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);

typedef IntrovertEngineStopC = FfiResult Function();
typedef IntrovertEngineStopDart = FfiResult Function();

typedef IntrovertGetPeerIdC = Pointer<Utf8> Function();
typedef IntrovertGetPeerIdDart = Pointer<Utf8> Function();

typedef IntrovertNetworkStartC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);
typedef IntrovertNetworkStartDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);

typedef IntrovertEconomyStartMonitoringC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);
typedef IntrovertEconomyStartMonitoringDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);

typedef IntrovertNetworkSendMessageC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkSendMessageDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertNetworkInitiateWebRtcC = FfiResult Function(Pointer<Utf8> peerId, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkInitiateWebRtcDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertNetworkAddAddressC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);
typedef IntrovertNetworkAddAddressDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);

typedef IntrovertClaimRewardsAsyncC = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);
typedef IntrovertClaimRewardsAsyncDart = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);

typedef IntrovertStoreMessageAsyncC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertStoreMessageAsyncDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertNetworkEstablishSecureSessionC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertNetworkEstablishSecureSessionDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertNetworkFetchMailboxC = FfiResult Function();
typedef IntrovertNetworkFetchMailboxDart = FfiResult Function();

typedef IntrovertNetworkStartMediaStreamC = FfiResult Function(Pointer<Utf8> peerId, Uint8 mediaType);
typedef IntrovertNetworkStartMediaStreamDart = FfiResult Function(Pointer<Utf8> peerId, int mediaType);

typedef IntrovertStorageGetContactsC = FfiResult Function();
typedef IntrovertStorageGetContactsDart = FfiResult Function();

typedef IntrovertStorageDeleteContactC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertStorageDeleteContactDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertStorageClearContactsC = FfiResult Function();
typedef IntrovertStorageClearContactsDart = FfiResult Function();

typedef IntrovertWormholeStartC = FfiResult Function();
typedef IntrovertWormholeStartDart = FfiResult Function();

typedef IntrovertWormholeJoinC = FfiResult Function(Pointer<Utf8> code);
typedef IntrovertWormholeJoinDart = FfiResult Function(Pointer<Utf8> code);

typedef IntrovertWebRtcCloseC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertWebRtcCloseDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertWebRtcRenegotiateC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertWebRtcRenegotiateDart = FfiResult Function(Pointer<Utf8> peerId);

// --- Event Models ---

class NetworkEvent {
  final int type;
  final Uint8List data;
  NetworkEvent(this.type, this.data);
}

class MediaFrameEvent {
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

// --- Main Client Implementation ---

class IntrovertClient {
  static final IntrovertClient _instance = IntrovertClient._internal();
  factory IntrovertClient() => _instance;

  late DynamicLibrary _dylib;
  
  late IntrovertGenerateMnemonicDart _generateMnemonic;
  late IntrovertFreeStringDart _freeString;
  late IntrovertFreeBinaryDart _freeBinary;
  late IntrovertMnemonicToSeedDart _mnemonicToSeed;
  late IntrovertEngineStartDart _engineStart;
  late IntrovertEngineStopDart _engineStop;
  late IntrovertGetPeerIdDart _getPeerId;
  late IntrovertNetworkStartDart _networkStart;
  late IntrovertEconomyStartMonitoringDart _economyStartMonitoring;
  late IntrovertNetworkSendMessageDart _networkSendMessage;
  late IntrovertNetworkInitiateWebRtcDart _networkInitiateWebRtc;
  late IntrovertNetworkAddAddressDart _addAddress;
  late IntrovertClaimRewardsAsyncDart _claimRewardsAsync;
  late IntrovertStoreMessageAsyncDart _storeMessageAsync;
  late IntrovertNetworkEstablishSecureSessionDart _establishSecureSession;
  late IntrovertNetworkFetchMailboxDart _fetchMailbox;
  late IntrovertNetworkStartMediaStreamDart _startMediaStream;
  late IntrovertStorageGetContactsDart _getContacts;
  late IntrovertStorageDeleteContactDart _deleteContact;
  late IntrovertStorageClearContactsDart _clearContacts;
  late IntrovertWormholeStartDart _wormholeStart;
  late IntrovertWormholeJoinDart _wormholeJoin;
  late IntrovertWebRtcCloseDart _closeWebRtc;
  late IntrovertWebRtcRenegotiateDart _renegotiateWebRtc;

  NativeCallable<NativeNetworkCallback>? _networkCallable;
  NativeCallable<NativeNetworkCallback>? _economyCallable;

  final StreamController<NetworkEvent> _networkStreamController = StreamController<NetworkEvent>.broadcast();
  Stream<NetworkEvent> get networkStream => _networkStreamController.stream;

  final StreamController<MediaFrameEvent> _mediaStreamController = StreamController<MediaFrameEvent>.broadcast();
  Stream<MediaFrameEvent> get mediaStream => _mediaStreamController.stream;

  final StreamController<Map<String, dynamic>> _economyStreamController = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get economyStream => _economyStreamController.stream;

  IntrovertClient._internal() {
    _loadLibrary();
    _bindFunctions();
  }

  void _loadLibrary() {
    if (Platform.isAndroid || Platform.isLinux) {
      _dylib = DynamicLibrary.open('libintrovert.so');
    } else if (Platform.isIOS || Platform.isMacOS) {
      _dylib = DynamicLibrary.process();
    } else {
      throw UnsupportedError('Unsupported platform.');
    }
  }

  void _bindFunctions() {
    _generateMnemonic = _dylib.lookupFunction<IntrovertGenerateMnemonicC, IntrovertGenerateMnemonicDart>('introvert_generate_mnemonic');
    _freeString = _dylib.lookupFunction<IntrovertFreeStringC, IntrovertFreeStringDart>('introvert_free_string');
    _freeBinary = _dylib.lookupFunction<IntrovertFreeBinaryC, IntrovertFreeBinaryDart>('introvert_free_binary');
    _mnemonicToSeed = _dylib.lookupFunction<IntrovertMnemonicToSeedC, IntrovertMnemonicToSeedDart>('introvert_mnemonic_to_seed');
    _engineStart = _dylib.lookupFunction<IntrovertEngineStartC, IntrovertEngineStartDart>('introvert_engine_start');
    _engineStop = _dylib.lookupFunction<IntrovertEngineStopC, IntrovertEngineStopDart>('introvert_engine_stop');
    _getPeerId = _dylib.lookupFunction<IntrovertGetPeerIdC, IntrovertGetPeerIdDart>('introvert_get_peer_id');
    _networkStart = _dylib.lookupFunction<IntrovertNetworkStartC, IntrovertNetworkStartDart>('introvert_network_start');
    _economyStartMonitoring = _dylib.lookupFunction<IntrovertEconomyStartMonitoringC, IntrovertEconomyStartMonitoringDart>('introvert_economy_start_monitoring');
    _networkSendMessage = _dylib.lookupFunction<IntrovertNetworkSendMessageC, IntrovertNetworkSendMessageDart>('introvert_network_send_message');
    _networkInitiateWebRtc = _dylib.lookupFunction<IntrovertNetworkInitiateWebRtcC, IntrovertNetworkInitiateWebRtcDart>('introvert_network_initiate_webrtc');
    _addAddress = _dylib.lookupFunction<IntrovertNetworkAddAddressC, IntrovertNetworkAddAddressDart>('introvert_network_add_address');
    _claimRewardsAsync = _dylib.lookupFunction<IntrovertClaimRewardsAsyncC, IntrovertClaimRewardsAsyncDart>('introvert_claim_rewards_async');
    _storeMessageAsync = _dylib.lookupFunction<IntrovertStoreMessageAsyncC, IntrovertStoreMessageAsyncDart>('introvert_store_message_async');
    _establishSecureSession = _dylib.lookupFunction<IntrovertNetworkEstablishSecureSessionC, IntrovertNetworkEstablishSecureSessionDart>('introvert_network_establish_secure_session');
    _fetchMailbox = _dylib.lookupFunction<IntrovertNetworkFetchMailboxC, IntrovertNetworkFetchMailboxDart>('introvert_network_fetch_mailbox');
    _startMediaStream = _dylib.lookupFunction<IntrovertNetworkStartMediaStreamC, IntrovertNetworkStartMediaStreamDart>('introvert_network_start_media_stream');
    _getContacts = _dylib.lookupFunction<IntrovertStorageGetContactsC, IntrovertStorageGetContactsDart>('introvert_storage_get_contacts');
    _deleteContact = _dylib.lookupFunction<IntrovertStorageDeleteContactC, IntrovertStorageDeleteContactDart>('introvert_storage_delete_contact');
    _clearContacts = _dylib.lookupFunction<IntrovertStorageClearContactsC, IntrovertStorageClearContactsDart>('introvert_storage_clear_contacts');
    _wormholeStart = _dylib.lookupFunction<IntrovertWormholeStartC, IntrovertWormholeStartDart>('introvert_wormhole_start');
    _wormholeJoin = _dylib.lookupFunction<IntrovertWormholeJoinC, IntrovertWormholeJoinDart>('introvert_wormhole_join');
    _closeWebRtc = _dylib.lookupFunction<IntrovertWebRtcCloseC, IntrovertWebRtcCloseDart>('introvert_webrtc_close_connection');
    _renegotiateWebRtc = _dylib.lookupFunction<IntrovertWebRtcRenegotiateC, IntrovertWebRtcRenegotiateDart>('introvert_webrtc_renegotiate');
  }

  void startNetwork() {
    if (_networkCallable != null) return;
    _networkCallable = NativeCallable<NativeNetworkCallback>.listener((int eventType, Pointer<Uint8> dataPtr, int dataLen) {
      if (dataPtr.address == 0) return;
      try {
        final Pointer<Uint8> castedPtr = dataPtr.cast<Uint8>();
        if (eventType == 5) {
          final header = castedPtr.cast<MediaFrameHeader>().ref;
          final headerSize = sizeOf<MediaFrameHeader>();
          _mediaStreamController.add(MediaFrameEvent(
            codec: header.codec,
            width: header.width,
            height: header.height,
            payload: Pointer<Uint8>.fromAddress(castedPtr.address + headerSize),
            payloadLen: dataLen - headerSize,
            basePtr: castedPtr,
            baseLen: dataLen,
          ));
        } else {
          final data = castedPtr.asTypedList(dataLen);
          _networkStreamController.add(NetworkEvent(eventType, Uint8List.fromList(data)));
        }
      } finally {
        if (eventType != 5) _freeBinary(dataPtr, dataLen);
      }
    });
    _handleFfiResult(_networkStart(_networkCallable!.nativeFunction), context: "Network Start");
  }

  void startEconomyMonitoring(void Function(Map<String, dynamic> stats) onUpdate) {
    _economyCallable?.close();
    _economyCallable = NativeCallable<NativeNetworkCallback>.listener((int eventType, Pointer<Uint8> dataPtr, int dataLen) {
      if (dataPtr.address == 0) return;
      try {
        if (eventType == 9) {
          final data = dataPtr.cast<Uint8>().asTypedList(dataLen);
          final stats = json.decode(utf8.decode(data)) as Map<String, dynamic>;
          stats['sol_balance'] = stats['intr_balance'] ?? stats['sol_balance'] ?? 0;
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

  void startEngine(Uint8List seed, String dbPath) {
    using((Arena arena) {
      final seedPtr = arena<Uint8>(32);
      for (var i = 0; i < 32; i++) seedPtr[i] = seed[i];
      _handleFfiResult(_engineStart(seedPtr, dbPath.toNativeUtf8(allocator: arena)), context: "Engine Start");
    });
  }

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
  Future<void> clearAllContacts() async => _handleFfiResult(_clearContacts(), context: "Clear Contacts");

  Future<void> sendMessage(String id, String msg) async {
    final comp = Completer<void>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) => res.code == 0 ? comp.complete() : comp.completeError(Exception("Send failed")));
    using((Arena arena) => _networkSendMessage(id.toNativeUtf8(allocator: arena), msg.toNativeUtf8(allocator: arena), cb.nativeFunction));
    await comp.future; cb.close();
  }

  Future<void> storeMessage(String id, String msg) async {
    final comp = Completer<void>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) => res.code == 0 ? comp.complete() : comp.completeError(Exception("Store failed")));
    using((Arena arena) => _storeMessageAsync(id.toNativeUtf8(allocator: arena), msg.toNativeUtf8(allocator: arena), cb.nativeFunction));
    await comp.future; cb.close();
  }

  void establishSecureSession(String id) => using((Arena arena) => _handleFfiResult(_establishSecureSession(id.toNativeUtf8(allocator: arena)), context: "Secure Session"));

  Future<void> initiateWebRtc(String peerId) async {
    final completer = Completer<void>();
    final callback = NativeCallable<NativeFfiCallback>.listener((FfiResult result) {
      if (result.code == 0) completer.complete();
      else completer.completeError(Exception("WebRTC error (${result.code})"));
    });
    using((Arena arena) => _networkInitiateWebRtc(peerId.toNativeUtf8(allocator: arena), callback.nativeFunction));
    await completer.future; callback.close();
  }

  void fetchMailbox() => _handleFfiResult(_fetchMailbox(), context: "Fetch Mailbox");
  void startMediaStream(String id, int type) => using((Arena arena) => _handleFfiResult(_startMediaStream(id.toNativeUtf8(allocator: arena), type), context: "Media Stream"));

  Future<String> claimRewards() async {
    final comp = Completer<String>();
    final cb = NativeCallable<NativeRewardCallback>.listener((int status, Pointer<Utf8> sigPtr) => status == 0 ? comp.complete(sigPtr.toDartString()) : comp.completeError(Exception("Claim error")));
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
      throw Exception('$context Error (${result.code}): $msg');
    }
  }
}
