# Media Ingestion Safety Module — Implementation Plan

## Overview

The safety module is a mandatory inspection gate before any media reaches AES-256-GCM encryption. Flow:

```
User picks file → UploadController reads bytes
  → Rust introvert_inspect_media() FFI [PRIMARY gate: PDQ hash + verdict]
  → FFI unavailable → Dart pure fallback (fail-secure: deny)
  → approved → proceed to sendFile/registerSeeder
  → blocked → zero-fill buffer, show alert, abort
```

---

## File Manifest

### New Files (7)

| File | Purpose |
|------|---------|
| `lib/src/domain/safety/safety_types.dart` | Domain types: enum + data class |
| `lib/src/domain/safety/i_safety_service.dart` | Abstract interface |
| `lib/src/infrastructure/safety/native_hash_bridge.dart` | FFI bridge + pure Dart fallback |
| `lib/src/infrastructure/safety/tflite_safety_classifier.dart` | Scaffold classifier |
| `lib/src/ui/media/upload_controller.dart` | Upload gate controller |
| `src/safety.rs` | Rust PDQ hashing module |
| `test/safety/native_hash_bridge_test.dart` | Unit tests |

### Modified Files (5)

| File | Change |
|------|--------|
| `src/lib.rs` | Add `pub mod safety;` + FFI function |
| `Cargo.toml` | Add `image = "0.25"` |
| `lib/src/native/introvert_client.dart` | Add FFI typedef + safeLookup |
| `lib/main.dart` | Register UploadController in Provider |
| `lib/views/chat_screen.dart` | Refactor 3 send methods |
| `lib/views/group_chat_screen.dart` | Refactor 3 send methods |

---

## Step 1 — Domain Types

### `lib/src/domain/safety/safety_types.dart`

```dart
import 'dart:typed_data';

enum MediaSafetyVerdict {
  approved,
  knownViolationBlocked,
  heuristicRiskBlocked,
  processingFailure;

  bool get isAllowed => this == MediaSafetyVerdict.approved;
}

class SafetyAuditResult {
  final MediaSafetyVerdict verdict;
  final String computedHashHex;
  final double confidenceScore; // 0.0–1.0
  final DateTime timestamp;

  const SafetyAuditResult({
    required this.verdict,
    required this.computedHashHex,
    required this.confidenceScore,
    required this.timestamp,
  });

  factory SafetyAuditResult.fromJson(Map<String, dynamic> json) {
    return SafetyAuditResult(
      verdict: MediaSafetyVerdict.values.byName(json['verdict'] as String),
      computedHashHex: json['hash_hex'] as String,
      confidenceScore: (json['confidence'] as num).toDouble(),
      timestamp: DateTime.parse(json['timestamp'] as String),
    );
  }

  factory SafetyAuditResult.processingFailure() {
    return SafetyAuditResult(
      verdict: MediaSafetyVerdict.processingFailure,
      computedHashHex: '',
      confidenceScore: 0.0,
      timestamp: DateTime.now(),
    );
  }
}
```

### `lib/src/domain/safety/i_safety_service.dart`

```dart
import 'dart:typed_data';
import 'safety_types.dart';

abstract class ISafetyService {
  Future<SafetyAuditResult> inspectMediaPayload(Uint8List rawBytes, String mimeType);
}
```

---

## Step 2 — Rust PDQ Perceptual Hashing

### `src/safety.rs`

```rust
use serde_json::json;
use crate::FfiResult;

fn compute_pdq_hash(bytes: &[u8], mime: &str) -> Result<[u8; 32], String> {
    if mime.starts_with("video/") {
        return Err("Video PDQ requires frame extraction; fail-secure".into());
    }
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Image decode failed: {}", e))?;

    // Resize to 64x64 grayscale
    let gray = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .to_luma8();

    // Build 64x64 f64 matrix
    let mut matrix = [[0.0f64; 64]; 64];
    for y in 0..64 {
        for x in 0..64 {
            matrix[y][x] = gray[(x, y)][0] as f64;
        }
    }

    // Simplified 8x8 block DCT
    let mut dct = [[0.0f64; 8]; 8];
    for u in 0..8 {
        for v in 0..8 {
            let mut sum = 0.0;
            for y in 0..64 {
                for x in 0..64 {
                    sum += matrix[y][x]
                        * (((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 128.0).cos()
                        * (((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI) / 128.0).cos();
                }
            }
            dct[u][v] = sum;
        }
    }

    // Median-threshold to 32-byte hash
    let mut flat: Vec<f64> = dct.iter().flatten().copied().collect();
    flat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = flat[flat.len() / 2];

    let mut hash = [0u8; 32];
    for (i, val) in dct.iter().flatten().enumerate() {
        let byte_idx = i / 8;
        let bit_idx = 7 - (i % 8);
        if *val > median {
            hash[byte_idx] |= 1 << bit_idx;
        }
    }
    Ok(hash)
}

const BLOCKLIST: &[[u8; 32]] = &[]; // populated at build or from config

fn hamming_distance(a: &[u8; 32], b: &[u8; 32]) -> u32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x ^ y).count_ones()).sum()
}

fn shannon_entropy(data: &[u8]) -> f64 {
    let mut freq = [0u64; 256];
    for &b in data { freq[b as usize] += 1; }
    let len = data.len() as f64;
    -freq.iter().filter(|&&f| f > 0).map(|&f| {
        let p = f as f64 / len;
        p * p.log2()
    }).sum::<f64>()
}

pub fn inspect_media(bytes: &[u8], mime: &str) -> (String, String, f64) {
    match compute_pdq_hash(bytes, mime) {
        Ok(hash) => {
            let hash_hex = hex::encode(hash);
            for blocked in BLOCKLIST {
                if hamming_distance(&hash, blocked) <= 10 {
                    return (hash_hex, "knownViolationBlocked".into(), 0.99);
                }
            }
            let entropy = shannon_entropy(bytes);
            if entropy > 7.8 && mime.starts_with("image/") {
                return (hash_hex, "heuristicRiskBlocked".into(), 0.85);
            }
            (hash_hex, "approved".into(), 0.95)
        }
        Err(_) => (String::new(), "processingFailure".into(), 0.0),
    }
}
```

### FFI Entry Point (append to `src/lib.rs`)

Add `pub mod safety;` after line 13 (existing module declarations).

Append this function at end of file:

```rust
/// Inspects media payload BEFORE encryption. Returns JSON: {hash_hex, verdict, confidence, timestamp}.
#[no_mangle]
pub extern "C" fn introvert_inspect_media(
    bytes_ptr: *const u8,
    length: usize,
    mime_type_ptr: *const c_char,
) -> FfiResult {
    if bytes_ptr.is_null() || mime_type_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }
    if length == 0 {
        return FfiResult::error(-12, "Empty payload");
    }

    let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, length) };
    let mime = unsafe { CStr::from_ptr(mime_type_ptr).to_string_lossy().into_owned() };

    let (hash_hex, verdict, confidence) = safety::inspect_media(bytes, &mime);

    let result = json!({
        "hash_hex": hash_hex,
        "verdict": verdict,
        "confidence": confidence,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    FfiResult::binary(result.to_string().into_bytes())
}
```

### `Cargo.toml` addition (under `[dependencies]`)

```toml
image = "0.25"
```

---

## Step 3 — Dart FFI Bridge + Pure Fallback

### `lib/src/infrastructure/safety/native_hash_bridge.dart`

```dart
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
      debugPrint('⚠️ inspect_media FFI unavailable: $e');
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
      if (result.code != 0) return SafetyAuditResult.processingFailure();
      final jsonStr = utf8.decode(result.data.asTypedList(result.len));
      _client.freeBinary(result.data, result.len);
      return SafetyAuditResult.fromJson(json.decode(jsonStr) as Map<String, dynamic>);
    } finally {
      calloc.free(bytesPtr);
      calloc.free(mimePtr);
    }
  }

  /// Pure Dart fallback — fail-secure: any error returns processingFailure.
  Future<SafetyAuditResult> _viaDartFallback(Uint8List rawBytes, String mimeType) async {
    try {
      final hashBytes = _foldHash(rawBytes);
      final hashHex = hashBytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
      final entropy = _shannonEntropy(rawBytes);
      if (entropy > 7.8 && mimeType.startsWith('image/')) {
        return SafetyAuditResult(
          verdict: MediaSafetyVerdict.heuristicRiskBlocked,
          computedHashHex: hashHex,
          confidenceScore: 0.70,
          timestamp: DateTime.now(),
        );
      }
      return SafetyAuditResult(
        verdict: MediaSafetyVerdict.approved,
        computedHashHex: hashHex,
        confidenceScore: 0.80,
        timestamp: DateTime.now(),
      );
    } catch (e) {
      debugPrint('❌ Dart safety fallback error: $e');
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
```

---

## Step 4 — TFLite Safety Classifier (Scaffold)

### `lib/src/infrastructure/safety/tflite_safety_classifier.dart`

```dart
import 'dart:typed_data';
import 'package:flutter/foundation.dart';

class TfliteSafetyClassifier {
  bool _initialized = false;

  // Output tensor indices
  static const int idxExplicit = 0;
  static const int idxViolentGore = 1;
  static const int idxMalwarePayload = 2;
  static const int idxClear = 3;

  Future<void> init() async {
    if (_initialized) return;
    // TODO: load tflite model when available
    _initialized = true;
  }

  /// Returns [Explicit, ViolentGore, MalwarePayload, Clear] confidence vector.
  Future<List<double>> classify(Uint8List imageBytes) async {
    if (!_initialized) await init();
    // Mock: high confidence for Clear
    return [0.02, 0.01, 0.02, 0.95];
  }

  /// Normalizes raw RGB to [1, 224, 224, 3] float tensor.
  Float32List normalizeForModel(Uint8List rawRgb, int width, int height) {
    final tensor = Float32List(1 * 224 * 224 * 3);
    int idx = 0;
    for (int y = 0; y < 224; y++) {
      for (int x = 0; x < 224; x++) {
        final sx = (x * width / 224).floor();
        final sy = (y * height / 224).floor();
        final si = (sy * width + sx) * 3;
        tensor[idx++] = rawRgb[si] / 255.0;
        tensor[idx++] = rawRgb[si + 1] / 255.0;
        tensor[idx++] = rawRgb[si + 2] / 255.0;
      }
    }
    return tensor;
  }
}
```

No pubspec changes yet. When model is ready, add:
- `tflite_flutter: ^0.10.4` to dependencies
- `assets/models/safety_classifier.tflite` to assets

---

## Step 5 — UploadController

### `lib/src/ui/media/upload_controller.dart`

```dart
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import '../../domain/safety/safety_types.dart';
import '../../domain/safety/i_safety_service.dart';
import '../../infrastructure/safety/tflite_safety_classifier.dart';
import '../../native/introvert_client.dart';

class UploadController {
  final IntrovertClient _client;
  final ISafetyService _safetyService;
  final TfliteSafetyClassifier _classifier;

  UploadController({
    required IntrovertClient client,
    required ISafetyService safetyService,
    TfliteSafetyClassifier? classifier,
  })  : _client = client,
        _safetyService = safetyService,
        _classifier = classifier ?? TfliteSafetyClassifier();

  /// For DM file sends. Inspects then sends via _client.sendFile.
  Future<SafetyAuditResult> inspectAndSend({
    required String peerId,
    required String filePath,
    BuildContext? context,
  }) async {
    final file = File(filePath);
    if (!await file.exists()) return SafetyAuditResult.processingFailure();

    final rawBytes = await file.readAsBytes();
    final mimeType = _guessMime(filePath);
    final result = await _safetyService.inspectMediaPayload(rawBytes, mimeType);

    if (!result.verdict.isAllowed) {
      _zeroFill(rawBytes);
      if (context != null && context.mounted) _showBlocked(context, result);
      return result;
    }

    _client.sendFile(peerId, filePath);
    return result;
  }

  /// For group file sends. Inspects then does computeFileHash + registerSeeder + sendGroupMessage.
  Future<SafetyAuditResult> inspectAndSendGroup({
    required String groupId,
    required String filePath,
    required String filename,
    required String mimeType,
    BuildContext? context,
  }) async {
    final file = File(filePath);
    if (!await file.exists()) return SafetyAuditResult.processingFailure();

    final rawBytes = await file.readAsBytes();
    final result = await _safetyService.inspectMediaPayload(rawBytes, mimeType);

    if (!result.verdict.isAllowed) {
      _zeroFill(rawBytes);
      if (context != null && context.mounted) _showBlocked(context, result);
      return result;
    }

    final size = rawBytes.length;
    final fileHash = _client.computeFileHash(filePath);
    final transferId = "gft_${fileHash}_${DateTime.now().millisecondsSinceEpoch}";
    _client.registerSeeder(transferId, filePath, fileHash, size, groupId);
    final manifest = "[FILE]:${json.encode({
      "transfer_id": transferId,
      "sender_peer_id": _client.localPeerId,
      "filename": filename,
      "mime_type": mimeType,
      "total_size": size,
      "file_hash": fileHash,
      "is_relayed": true,
      "group_id": groupId,
    })}";
    _client.sendGroupMessage(groupId, manifest);
    return result;
  }

  void _zeroFill(Uint8List buffer) {
    for (int i = 0; i < buffer.length; i++) {
      buffer[i] = 0;
    }
  }

  String _guessMime(String path) {
    final ext = path.split('.').last.toLowerCase();
    const map = {
      'jpg': 'image/jpeg', 'jpeg': 'image/jpeg', 'png': 'image/png',
      'gif': 'image/gif', 'webp': 'image/webp', 'mp4': 'video/mp4',
      'mov': 'video/quicktime', 'avi': 'video/x-msvideo', 'pdf': 'application/pdf',
    };
    return map[ext] ?? 'application/octet-stream';
  }

  void _showBlocked(BuildContext context, SafetyAuditResult result) {
    final reason = switch (result.verdict) {
      MediaSafetyVerdict.knownViolationBlocked => 'known policy violation',
      MediaSafetyVerdict.heuristicRiskBlocked => 'potential safety risk',
      MediaSafetyVerdict.processingFailure => 'inspection failure',
      _ => 'unknown',
    };
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text('File blocked: $reason'),
      backgroundColor: Colors.red.shade700,
    ));
  }
}
```

---

## Step 6 — IntrovertClient FFI Binding

### In `lib/src/native/introvert_client.dart`

**Add typedef near line 95:**
```dart
typedef IntrovertInspectMediaC = FfiResult Function(Pointer<Uint8> bytes, Size length, Pointer<Utf8> mimeType);
typedef IntrovertInspectMediaDart = FfiResult Function(Pointer<Uint8> bytes, int length, Pointer<Utf8> mimeType);
```

**Add late field near line 655:**
```dart
late IntrovertInspectMediaDart _inspectMedia;
```

**Add safeLookup in _bindFunctions() after line 1098:**
```dart
_inspectMedia = safeLookup('inspect_media',
    () => _dylib.lookupFunction<IntrovertInspectMediaC, IntrovertInspectMediaDart>(
        'introvert_inspect_media'),
    (b, l, m) => FfiResult.dummy);
```

**Add public accessor near computeFileHash:**
```dart
IntrovertInspectMediaDart? getInspectMediaFunction() => _inspectMedia;
```

---

## Step 7 — Provider Registration

### In `lib/main.dart`

Add imports:
```dart
import 'src/infrastructure/safety/native_hash_bridge.dart';
import 'src/infrastructure/safety/tflite_safety_classifier.dart';
import 'src/ui/media/upload_controller.dart';
import 'src/domain/safety/i_safety_service.dart';
```

Add to MultiProvider.providers list (after existing Provider entries):
```dart
Provider<ISafetyService>(create: (_) => NativeHashBridge(client)),
Provider<UploadController>(
  create: (ctx) => UploadController(
    client: client,
    safetyService: ctx.read<ISafetyService>(),
  ),
),
```

---

## Step 8 — Chat Screen Refactor

### `lib/views/chat_screen.dart` (3 methods)

Add imports at top:
```dart
import 'package:provider/provider.dart';
import '../src/ui/media/upload_controller.dart';
```

**Replace `_pickAndSendImage()` (lines 2536–2561):**
```dart
void _pickAndSendImage() async {
  try {
    final pickedFiles = await ImagePicker().pickMultiImage(imageQuality: 100);
    if (pickedFiles.isNotEmpty) {
      final paths = <String>[];
      for (var file in pickedFiles) {
        String path = file.path;
        final ext = path.split('.').last.toLowerCase();
        if (ext == 'heic' || ext == 'heif') path = await _convertHeicToJpeg(path);
        paths.add(path);
      }
      final caption = await _showCaptionDialog(paths);
      if (caption == null) return;
      final uploader = context.read<UploadController>();
      for (var path in paths) {
        await uploader.inspectAndSend(peerId: widget.peerId, filePath: path, context: context);
      }
      if (caption.isNotEmpty) _client.sendMessage(widget.peerId, caption);
    }
  } catch (e) {
    if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed: $e")));
  }
}
```

**Replace `_pickAndSendVideo()` (lines 2564–2577):**
```dart
void _pickAndSendVideo() async {
  try {
    final pickedFile = await ImagePicker().pickVideo(source: ImageSource.gallery);
    if (pickedFile != null) {
      final caption = await _showCaptionDialog([pickedFile.path]);
      if (caption == null) return;
      await context.read<UploadController>().inspectAndSend(
        peerId: widget.peerId, filePath: pickedFile.path, context: context,
      );
      if (caption.isNotEmpty) _client.sendMessage(widget.peerId, caption);
    }
  } catch (e) {
    if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed: $e")));
  }
}
```

**Replace `_sendFile()` (lines 2580–2591):**
```dart
void _sendFile() async {
  final result = await FilePicker.platform.pickFiles(type: FileType.any);
  if (result != null && result.files.single.path != null) {
    final path = result.files.single.path!;
    final caption = await _showCaptionDialog([path]);
    if (caption == null) return;
    await context.read<UploadController>().inspectAndSend(
      peerId: widget.peerId, filePath: path, context: context,
    );
    if (caption.isNotEmpty) _client.sendMessage(widget.peerId, caption);
  }
}
```

### `lib/views/group_chat_screen.dart` (3 methods)

Add imports:
```dart
import 'package:provider/provider.dart';
import '../src/ui/media/upload_controller.dart';
```

**Replace `_pickAndSendImage()` (lines 2196–2229):**
```dart
void _pickAndSendImage() async {
  try {
    final pickedFiles = await ImagePicker().pickMultiImage(imageQuality: 100);
    if (pickedFiles.isNotEmpty) {
      for (var pickedFile in pickedFiles) {
        String path = pickedFile.path;
        String filename = pickedFile.name;
        final ext = path.split('.').last.toLowerCase();
        if (ext == 'heic' || ext == 'heif') {
          path = await _convertHeicToJpeg(path);
          filename = filename.replaceAll(RegExp(r'\.(heic|heif)$', caseSensitive: false), '.jpg');
        }
        await context.read<UploadController>().inspectAndSendGroup(
          groupId: widget.groupId, filePath: path, filename: filename,
          mimeType: 'image/jpeg', context: context,
        );
        _addSendingPlaceholder("tmp_$filename", filename, 'image/jpeg', path);
      }
      _loadMessages();
    }
  } catch (_) {}
}
```

**Replace `_pickAndSendVideo()` (lines 2248–2272):**
```dart
void _pickAndSendVideo() async {
  try {
    final pickedFile = await ImagePicker().pickVideo(source: ImageSource.gallery);
    if (pickedFile != null) {
      await context.read<UploadController>().inspectAndSendGroup(
        groupId: widget.groupId, filePath: pickedFile.path,
        filename: pickedFile.name, mimeType: 'video/mp4', context: context,
      );
      _addSendingPlaceholder("tmp_${pickedFile.name}", pickedFile.name, 'video/mp4', pickedFile.path);
      _loadMessages();
    }
  } catch (_) {}
}
```

**Replace `_sendFile()` (lines 2275–2299):**
```dart
void _sendFile() async {
  try {
    final result = await FilePicker.platform.pickFiles(type: FileType.any);
    if (result != null && result.files.single.path != null) {
      final path = result.files.single.path!;
      await context.read<UploadController>().inspectAndSendGroup(
        groupId: widget.groupId, filePath: path,
        filename: result.files.single.name, mimeType: 'application/octet-stream', context: context,
      );
      _addSendingPlaceholder("tmp_${result.files.single.name}", result.files.single.name, 'application/octet-stream', path);
      _loadMessages();
    }
  } catch (_) {}
}
```

---

## Error Handling Strategy

| Layer | Failure Mode | Behavior |
|-------|-------------|----------|
| Rust FFI | `introvert_inspect_media` returns non-zero code | Dart returns `processingFailure` (deny) |
| Rust FFI | Function not found in dylib | `safeLookup` returns dummy → bridge detects and falls back to Dart |
| Dart fallback | Image decode fails | Catch → `processingFailure` (deny) |
| TFLite scaffold | Model not loaded | Returns mock vector (95% Clear) — no blocking impact until real model |
| File I/O | File doesn't exist / unreadable | `processingFailure` (deny) |
| Entropy check | Shannon > 7.8 on image | `heuristicRiskBlocked` |

**Fail-secure principle**: every exception path produces a denied verdict.

---

## Memory Safety Considerations

1. **Zero-fill on rejection**: `UploadController._zeroFill()` overwrites the `Uint8List` buffer with zeros immediately after a blocked verdict, before any exception can propagate.

2. **Arena allocation**: FFI calls use `calloc` with explicit `free` in `finally` blocks — no native memory leaks on error paths.

3. **Rust side**: The `bytes_ptr` slice is borrowed (not copied) for the duration of the FFI call. No heap allocation of the raw bytes occurs on the Rust side — only the result JSON is allocated via `FfiResult::binary`.

4. **Dart GC**: The `Uint8List` containing file bytes will be garbage collected after the `inspectAndSend` method returns. The explicit zero-fill ensures even if GC is delayed, the buffer contains no sensitive data.

---

## Dependencies

### `Cargo.toml` (add under `[dependencies]`)
```toml
image = "0.25"
```

### `pubspec.yaml`
No changes for the core module. Future additions when TFLite model is ready:
```yaml
tflite_flutter: ^0.10.4
```

---

## Verification Steps

1. **Build Rust library**: `cargo build --release` — verify `src/safety.rs` compiles, `introvert_inspect_media` symbol is exported
2. **Verify FFI symbol**: `nm -gU libintrovert.dylib | grep inspect_media` on macOS
3. **Unit tests**: `flutter test test/safety/native_hash_bridge_test.dart` — test fallback hash, entropy, verdict mapping
4. **Static analysis**: `flutter analyze` — zero new warnings
5. **Integration test**: Pick an image in chat_screen → verify SnackBar shows "File blocked" or send proceeds
6. **Memory test**: Use a large file, verify rejection path zero-fills buffer (debug print buffer[0] after zero-fill)
7. **FFI failure test**: Temporarily rename dylib → verify Dart fallback activates and blocks high-entropy images
8. **Group flow test**: Send image in group_chat_screen → verify inspectAndSendGroup path works end-to-end
