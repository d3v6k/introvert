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
