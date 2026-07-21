# Media Safety Module — Full Implementation Details

**Version:** 1.0
**Last Updated:** 2026-07-21
**Status:** Active (TFLite model integration pending)

---

## Overview

The Media Safety Module is an on-device content inspection system that validates all media files **before** they enter the Introvert mesh network. Every image, video, and file selected for sending passes through this module before AES-256-GCM encryption and P2P transmission.

**No content, hashes, or metadata are transmitted to external servers.** All analysis runs entirely on the user's device.

---

## Architecture

```
User selects media
       │
       ▼
┌─────────────────────┐
│   UploadController   │  Dart — intercepts all 6 send call sites
│   (upload_controller │
│    .dart)            │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   native_hash_bridge │  Dart FFI — calls Rust inspect_media()
│   .dart              │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   safety.rs          │  Rust — PDQ hash, magic bytes, entropy
│   inspect_media()    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   TFLite Classifier  │  Dart — on-device ML inference
│   (pending model)    │
└─────────┬───────────┘
          │
          ▼
    ┌───────────┐
    │  Verdict  │
    └─────┬─────┘
          │
    ┌─────┴─────┐
    │           │
 Approved    Blocked
    │           │
    ▼           ▼
 Encrypt &   Rejected
 send P2P    locally
```

---

## Detection Layers

### Layer 1: Executable Masquerade Detection

**File:** `src/safety.rs` — `is_executable_masquerading()`

Checks if a file claiming to be an image/video is actually an executable binary by inspecting magic bytes:

| Format | Magic Bytes | Detection |
|--------|------------|-----------|
| PE (Windows) | `4D 5A` (MZ) | File starts with MZ but claims to be image/video |
| ELF (Linux) | `7F 45 4C 46` | File starts with ELF header but claims to be image/video |
| Mach-O (macOS) | `FEEDFACE` / `FEEDFACF` / `CEFAEDFE` / `CFFAEDFE` | File starts with Mach-O header but claims to be image/video |

**Verdict:** `knownViolationBlocked` (confidence 0.99) — hard block, never enters mesh.

### Layer 2: PDQ Perceptual Hash

**File:** `src/safety.rs` — `compute_pdq_hash()`

A custom implementation of Facebook's PDQ (Perceptual hash for Duplicate and quality) algorithm:

1. **Decode:** Load image from memory using the `image` crate
2. **Resize:** Scale to 64×64 grayscale using Lanczos3 resampling
3. **DCT:** Compute 8×8 block Discrete Cosine Transform on the 64×64 matrix
4. **Threshold:** Median-threshold the 64 DCT coefficients to produce a 256-bit (32-byte) hash
5. **Compare:** Hamming distance ≤ 10 against the bundled blocklist

**Blocklist:** `const BLOCKLIST: &[[u8; 32]] = &[]` — populated at build time or from configuration. Contains perceptual hashes of known CSAM and illegal imagery.

**Verdict:** `knownViolationBlocked` (confidence 0.99) if hash matches blocklist within hamming distance 10.

**Note:** Video PDQ requires frame extraction and is not yet implemented — videos fail open (approved with lower confidence 0.80).

### Layer 3: Shannon Entropy Analysis

**File:** `src/safety.rs` — `shannon_entropy()`

Calculates byte-frequency Shannon entropy of the file:

- **Threshold:** 7.95 bits/byte for image files
- **Purpose:** Detects steganography, encrypted payloads hidden in image data, and suspiciously high-entropy files
- **Mode:** Passive logging only — does NOT block. High-entropy images are allowed through with a warning log.

### Layer 4: TFLite On-Device Classifier

**File:** `lib/src/infrastructure/safety/tflite_safety_classifier.dart`

TensorFlow Lite classifier for content moderation:

| Output Index | Category | Description |
|-------------|----------|-------------|
| 0 | Explicit | Sexually explicit content |
| 1 | ViolentGore | Violence and graphic content |
| 2 | MalwarePayload | Embedded malware signatures |
| 3 | Clear | No violations detected |

**Input:** 224×224×3 RGB float tensor (normalized 0-1)
**Status:** Scaffold implemented, model loading marked as TODO. Currently returns mock `[0.02, 0.01, 0.02, 0.95]` (high confidence Clear).

**Dependencies:** `tflite_flutter` package (model file to be bundled with app)

---

## Verdict Types

**File:** `lib/src/domain/safety/safety_types.dart`

| Verdict | Meaning | Action |
|---------|---------|--------|
| `approved` | No violations detected | File proceeds to encryption and P2P send |
| `knownViolationBlocked` | Matched blocklist or executable masquerade | File rejected locally, never enters mesh |
| `heuristicRiskBlocked` | ML classifier flagged content (future) | File rejected locally |
| `processingFailure` | Hash computation or classification error | Fail-open: file proceeds with lower confidence |

---

## Upload Gate Integration

**File:** `lib/src/ui/media/upload_controller.dart`

The `UploadController` intercepts media at all 6 send call sites:

1. 1:1 chat image send
2. 1:1 chat file send
3. Group chat image send
4. Group chat file send
5. Forward/share media
6. Voice memo with attachment

Before any file is passed to the Rust encryption layer, the controller calls `inspectMediaPayload()` and blocks the send if the verdict is `knownViolationBlocked` or `heuristicRiskBlocked`.

---

## Source Files

| File | Language | Purpose |
|------|----------|---------|
| `src/safety.rs` | Rust | PDQ hash, entropy analysis, executable detection, `inspect_media()` entry point |
| `lib/src/domain/safety/safety_types.dart` | Dart | `MediaSafetyVerdict` enum, `SafetyAuditResult` class |
| `lib/src/domain/safety/i_safety_service.dart` | Dart | `ISafetyService` abstract interface |
| `lib/src/infrastructure/safety/native_hash_bridge.dart` | Dart | FFI bridge to Rust `inspect_media()` |
| `lib/src/infrastructure/safety/tflite_safety_classifier.dart` | Dart | TFLite classifier (model loading pending) |
| `lib/src/ui/media/upload_controller.dart` | Dart | Upload gate — intercepts all 6 send paths |

---

## Libraries & Crates

| Component | Library | Purpose |
|-----------|---------|---------|
| Image decode/resize | `image` (Rust crate) | Load from memory, resize to 64×64, convert to grayscale |
| Perceptual hashing | Custom Rust (`safety.rs`) | 8×8 block DCT + median threshold → 256-bit hash |
| Entropy calculation | Custom Rust (`safety.rs`) | Shannon entropy on raw bytes |
| Magic byte detection | Custom Rust (`safety.rs`) | PE/ELF/Mach-O header inspection |
| Hash encoding | `hex` (Rust crate) | Convert 32-byte hash to hex string |
| ML inference | `tflite_flutter` (Dart package) | On-device TensorFlow Lite classification (pending) |
| FFI bridge | `dart:ffi` + `ffi` (Dart package) | Call Rust `inspect_media()` from Dart |

---

## Privacy Guarantees

1. **No cloud APIs** — all analysis runs on-device
2. **No external lookups** — blocklist is bundled with the app binary
3. **No hash transmission** — PDQ hashes are computed and compared locally, never sent to any server
4. **No metadata leakage** — file content, hashes, entropy values, and classification results stay on-device
5. **Blocklist updates** — distributed via app releases, not runtime network calls
6. **Fail-open on errors** — if hashing or classification fails, the file is allowed through with reduced confidence (0.80), not blocked
