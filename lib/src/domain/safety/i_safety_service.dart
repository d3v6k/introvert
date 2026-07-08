import 'dart:typed_data';
import 'safety_types.dart';

abstract class ISafetyService {
  Future<SafetyAuditResult> inspectMediaPayload(Uint8List rawBytes, String mimeType);
}
