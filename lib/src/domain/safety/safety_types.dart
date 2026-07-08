
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
  final double confidenceScore;
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
