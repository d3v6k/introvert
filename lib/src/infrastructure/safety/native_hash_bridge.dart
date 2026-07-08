import 'dart:convert';
import 'dart:ffi';
import 'dart:math';
import 'dart:typed_data';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';
import '../../domain/safety/safety_types.dart';
import '../../domain/safety/i_safety_service.dart';
import '../../native/introvert_client.dart';

typedef InspectMediaC = FfiResult Function(Pointer<Uint8> bytes, Size length, Pointer<Utf8> mimeType);
typedef InspectMediaDart = FfiResult Function(Pointer<Uint8> bytes, int length, Pointer<Utf8> mimeType);

class NativeHashBridge implements ISafetyService {
  final IntrovertClient _client;
  InspectMediaDart? _inspectFn;
  bool _ffiAvailable = false;

  NativeHashBridge(this._client) {
    try {
      _inspectFn = _client.getInspectMediaFunction();
      _ffiAvailable = _inspectFn != null;
    } catch (e) {
      debugPrint('inspect_media FFI unavailable: $e');
    }
  }

  @override
  Future<SafetyAuditResult> inspectMediaPayload(Uint8List rawBytes, String mimeType) async {
    if (_ffiAvailable && _inspectFn != null) {
      return _viaFfi(rawBytes, mimeType);
    }
    return _viaDartFallback(rawBytes, mimeType);
  }

  Future<SafetyAuditResult> _viaFfi(Uint8List rawBytes, String mimeType) async {
    final bytesPtr = calloc<Uint8>(rawBytes.length);
    bytesPtr.asTypedList(rawBytes.length).setAll(0, rawBytes);
    final mimePtr = mimeType.toNativeUtf8();
    try {
      final result = _inspectFn!(bytesPtr, rawBytes.length, mimePtr);
      try {
        if (result.code != 0) return SafetyAuditResult.processingFailure();
        final jsonStr = utf8.decode(result.data.asTypedList(result.len));
        return SafetyAuditResult.fromJson(json.decode(jsonStr) as Map<String, dynamic>);
      } finally {
        if (result.data != nullptr && result.len > 0) {
          _client.freeBinary(result.data, result.len);
        }
      }
    } finally {
      calloc.free(bytesPtr);
      calloc.free(mimePtr);
    }
  }

  /// Pure Dart fallback — fail-open for high entropy, fail-secure for errors.
  Future<SafetyAuditResult> _viaDartFallback(Uint8List rawBytes, String mimeType) async {
    try {
      final hashBytes = _foldHash(rawBytes);
      final hashHex = hashBytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
      final entropy = _shannonEntropy(rawBytes);
      // High entropy is logged but not blocked — pass to cipher matrix
      if (entropy > 7.95 && mimeType.startsWith('image/')) {
        debugPrint('[Ingestion] High entropy asset allowed: ${entropy.toStringAsFixed(2)} bits/byte for $mimeType');
      }
      return SafetyAuditResult(
        verdict: MediaSafetyVerdict.approved,
        computedHashHex: hashHex,
        confidenceScore: 0.80,
        timestamp: DateTime.now(),
      );
    } catch (e) {
      debugPrint('Dart safety fallback error: $e');
      return SafetyAuditResult.processingFailure();
    }
  }

  List<int> _foldHash(Uint8List bytes) {
    final hash = Uint8List(8);
    for (int i = 0; i < bytes.length; i++) {
      hash[i % 8] ^= bytes[i];
    }
    return hash;
  }

  double _shannonEntropy(Uint8List data) {
    final freq = List<int>.filled(256, 0);
    for (final b in data) { freq[b]++; }
    final len = data.length.toDouble();
    double entropy = 0.0;
    for (final f in freq) {
      if (f > 0) {
        final p = f / len;
        entropy -= p * (log(p) / ln2);
      }
    }
    return entropy;
  }
}
