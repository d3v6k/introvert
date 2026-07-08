import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import '../../domain/safety/safety_types.dart';
import '../../domain/safety/i_safety_service.dart';
import '../../native/introvert_client.dart';

class UploadController {
  final IntrovertClient _client;
  final ISafetyService _safetyService;
  UploadController({
    required IntrovertClient client,
    required ISafetyService safetyService,
  })  : _client = client,
        _safetyService = safetyService;

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
    buffer.fillRange(0, buffer.length, 0);
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
      content: Text('File processing error: Asset failed compliance check ($reason)'),
      backgroundColor: Colors.red.shade700,
    ));
  }
}
