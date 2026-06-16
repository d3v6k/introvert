import 'dart:async';
import 'dart:math';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

/// Monitors network quality and provides adaptive call quality decisions.
class NetworkQualityService {
  static NetworkQualityService? _instance;
  static NetworkQualityService get instance => _instance ??= NetworkQualityService._();
  NetworkQualityService._();

  Timer? _monitorTimer;
  NetworkQuality _currentQuality = NetworkQuality.unknown;
  final StreamController<NetworkQuality> _qualityController = StreamController<NetworkQuality>.broadcast();
  final StreamController<NetworkQualityEvent> _eventController = StreamController<NetworkQualityEvent>.broadcast();

  Stream<NetworkQuality> get qualityStream => _qualityController.stream;
  Stream<NetworkQualityEvent> get eventStream => _eventController.stream;
  NetworkQuality get currentQuality => _currentQuality;

  final List<double> _recentBitrates = [];
  final List<double> _recentPacketLosses = [];
  final List<double> _recentRtts = [];
  static const int _maxSamples = 15;

  static const double _videoMinBandwidth = 300;
  static const double _videoRecommended = 1000;
  static const double _audioMinBandwidth = 40;
  static const double _audioRecommended = 100;
  static const double _maxPacketLossPercent = 5.0;
  static const double _maxRttMs = 500.0;
  static const double _criticalPacketLossPercent = 15.0;
  static const double _criticalRttMs = 1000.0;

  Future<PreCallCheckResult> checkNetworkForCall(bool wantsVideo) async {
    final connectivityResults = await Connectivity().checkConnectivity();
    final connectivity = connectivityResults.isNotEmpty ? connectivityResults.first : ConnectivityResult.none;

    if (connectivity == ConnectivityResult.none) {
      return PreCallCheckResult(
        allowed: false,
        reason: 'No network connection detected. Please connect to WiFi or mobile data.',
        suggestedMediaType: 0,
        quality: NetworkQuality.none,
      );
    }

    final estimatedBandwidth = _estimateBandwidth(connectivity);

    if (wantsVideo) {
      if (estimatedBandwidth < _videoMinBandwidth) {
        if (estimatedBandwidth >= _audioMinBandwidth) {
          return PreCallCheckResult(
            allowed: true,
            reason: 'Network too slow for video (${estimatedBandwidth.round()} kbps). Switching to audio only.',
            suggestedMediaType: 0,
            quality: _qualityFromBandwidth(estimatedBandwidth),
          );
        } else {
          return PreCallCheckResult(
            allowed: false,
            reason: 'Network too weak for calls (${estimatedBandwidth.round()} kbps). Min ${_audioMinBandwidth.round()} kbps required. Wait for better connectivity.',
            suggestedMediaType: 0,
            quality: _qualityFromBandwidth(estimatedBandwidth),
          );
        }
      } else if (estimatedBandwidth < _videoRecommended) {
        return PreCallCheckResult(
          allowed: true,
          reason: 'Starting video call on ${connectivity == ConnectivityResult.wifi ? 'WiFi' : 'mobile data'} (limited bandwidth). Quality may be reduced.',
          suggestedMediaType: 2,
          quality: _qualityFromBandwidth(estimatedBandwidth),
        );
      } else {
        return PreCallCheckResult(allowed: true, reason: '', suggestedMediaType: 2, quality: _qualityFromBandwidth(estimatedBandwidth));
      }
    } else {
      if (estimatedBandwidth < _audioMinBandwidth) {
        return PreCallCheckResult(
          allowed: false,
          reason: 'Network too weak for audio calls (${estimatedBandwidth.round()} kbps). Min ${_audioMinBandwidth.round()} kbps required.',
          suggestedMediaType: 0,
          quality: _qualityFromBandwidth(estimatedBandwidth),
        );
      }
      return PreCallCheckResult(allowed: true, reason: '', suggestedMediaType: 0, quality: _qualityFromBandwidth(estimatedBandwidth));
    }
  }

  void startMonitoring(RTCPeerConnection peerConnection) {
    _monitorTimer?.cancel();
    _recentBitrates.clear();
    _recentPacketLosses.clear();
    _recentRtts.clear();

    _monitorTimer = Timer.periodic(const Duration(seconds: 3), (_) async {
      try {
        final stats = await peerConnection.getStats();
        _processStats(stats);
      } catch (e) {
        debugPrint('[NetworkQuality] Error getting stats: $e');
      }
    });
  }

  void startGroupMonitoring(List<RTCPeerConnection> connections) {
    _monitorTimer?.cancel();
    _recentBitrates.clear();
    _recentPacketLosses.clear();
    _recentRtts.clear();

    _monitorTimer = Timer.periodic(const Duration(seconds: 3), (_) async {
      double totalBitrate = 0;
      double maxPacketLoss = 0;
      double maxRtt = 0;
      int validConnections = 0;

      for (final pc in connections) {
        try {
          final stats = await pc.getStats();
          final metrics = _extractMetrics(stats);
          if (metrics != null) {
            totalBitrate += metrics['bitrate'] ?? 0;
            maxPacketLoss = max(maxPacketLoss, metrics['packetLoss'] ?? 0);
            maxRtt = max(maxRtt, metrics['rtt'] ?? 0);
            validConnections++;
          }
        } catch (_) {}
      }

      if (validConnections > 0) {
        _recordSample(totalBitrate / validConnections, maxPacketLoss, maxRtt);
        _evaluateQuality();
      }
    });
  }

  void stopMonitoring() {
    _monitorTimer?.cancel();
    _monitorTimer = null;
  }

  void _processStats(List<StatsReport> reports) {
    final metrics = _extractMetrics(reports);
    if (metrics != null) {
      _recordSample(metrics['bitrate'] ?? 0, metrics['packetLoss'] ?? 0, metrics['rtt'] ?? 0);
      _evaluateQuality();
    }
  }

  Map<String, double>? _extractMetrics(List<StatsReport> reports) {
    double bitrate = 0;
    double packetLoss = 0;
    double rtt = 0;

    for (final report in reports) {
      final type = report.type;
      final values = report.values;

      if (type == 'inbound-rtp' || type == 'outbound-rtp') {
        final bytes = (values['bytesReceived'] ?? values['bytesSent'] ?? 0) as num;
        bitrate += (bytes.toDouble() * 8) / 1000;
        bitrate /= 3;

        final packetsLost = (values['packetsLost'] ?? 0) as num;
        final packetsReceived = (values['packetsReceived'] ?? values['packetsSent'] ?? 1) as num;
        if (packetsReceived > 0) {
          final loss = (packetsLost.toDouble() / (packetsReceived.toDouble() + packetsLost.toDouble())) * 100;
          packetLoss = max(packetLoss, loss);
        }
      }

      if (type == 'candidate-pair' && values['state'] == 'succeeded') {
        final currentRtt = (values['currentRoundTripTime'] ?? 0) as num;
        rtt = max(rtt, currentRtt.toDouble() * 1000);
      }
    }

    return {'bitrate': bitrate, 'packetLoss': packetLoss, 'rtt': rtt};
  }

  void _recordSample(double bitrate, double packetLoss, double rtt) {
    _recentBitrates.add(bitrate);
    _recentPacketLosses.add(packetLoss);
    _recentRtts.add(rtt);

    if (_recentBitrates.length > _maxSamples) _recentBitrates.removeAt(0);
    if (_recentPacketLosses.length > _maxSamples) _recentPacketLosses.removeAt(0);
    if (_recentRtts.length > _maxSamples) _recentRtts.removeAt(0);
  }

  void _evaluateQuality() {
    final avgBitrate = _average(_recentBitrates);
    final avgPacketLoss = _average(_recentPacketLosses);
    final avgRtt = _average(_recentRtts);

    NetworkQuality newQuality;

    if (avgPacketLoss >= _criticalPacketLossPercent || avgRtt >= _criticalRttMs) {
      newQuality = NetworkQuality.critical;
    } else if (avgPacketLoss >= _maxPacketLossPercent || avgRtt >= _maxRttMs) {
      newQuality = NetworkQuality.poor;
    } else if (avgBitrate < _audioMinBandwidth) {
      newQuality = NetworkQuality.poor;
    } else if (avgBitrate < _videoMinBandwidth) {
      newQuality = NetworkQuality.fair;
    } else if (avgBitrate < _videoRecommended) {
      newQuality = NetworkQuality.good;
    } else {
      newQuality = NetworkQuality.excellent;
    }

    if (newQuality != _currentQuality) {
      final previousQuality = _currentQuality;
      _currentQuality = newQuality;
      _qualityController.add(newQuality);

      if (previousQuality != NetworkQuality.unknown) {
        _eventController.add(NetworkQualityEvent(
          previousQuality: previousQuality,
          currentQuality: newQuality,
          shouldDowngradeVideo: _shouldDowngradeVideo(previousQuality, newQuality),
          shouldBlockCalls: newQuality == NetworkQuality.critical || newQuality == NetworkQuality.none,
          bitrate: avgBitrate,
          packetLoss: avgPacketLoss,
          rtt: avgRtt,
        ));
      }
    }
  }

  bool _shouldDowngradeVideo(NetworkQuality from, NetworkQuality to) {
    if ((from == NetworkQuality.excellent || from == NetworkQuality.good) &&
        (to == NetworkQuality.fair || to == NetworkQuality.poor)) {
      return true;
    }
    return false;
  }

  double _estimateBandwidth(ConnectivityResult connectivity) {
    switch (connectivity) {
      case ConnectivityResult.wifi:
        return 5000;
      case ConnectivityResult.mobile:
        return 1500;
      case ConnectivityResult.ethernet:
        return 10000;
      case ConnectivityResult.bluetooth:
        return 100;
      default:
        return 0;
    }
  }

  NetworkQuality _qualityFromBandwidth(double kbps) {
    if (kbps >= _videoRecommended) return NetworkQuality.excellent;
    if (kbps >= _videoMinBandwidth) return NetworkQuality.good;
    if (kbps >= _audioRecommended) return NetworkQuality.fair;
    if (kbps >= _audioMinBandwidth) return NetworkQuality.poor;
    return NetworkQuality.critical;
  }

  double _average(List<double> values) {
    if (values.isEmpty) return 0;
    return values.reduce((a, b) => a + b) / values.length;
  }

  void dispose() {
    _monitorTimer?.cancel();
    _qualityController.close();
    _eventController.close();
  }
}

enum NetworkQuality { unknown, none, critical, poor, fair, good, excellent }

class PreCallCheckResult {
  final bool allowed;
  final String reason;
  final int suggestedMediaType;
  final NetworkQuality quality;

  const PreCallCheckResult({
    required this.allowed,
    required this.reason,
    required this.suggestedMediaType,
    required this.quality,
  });
}

class NetworkQualityEvent {
  final NetworkQuality previousQuality;
  final NetworkQuality currentQuality;
  final bool shouldDowngradeVideo;
  final bool shouldBlockCalls;
  final double bitrate;
  final double packetLoss;
  final double rtt;

  const NetworkQualityEvent({
    required this.previousQuality,
    required this.currentQuality,
    required this.shouldDowngradeVideo,
    required this.shouldBlockCalls,
    required this.bitrate,
    required this.packetLoss,
    required this.rtt,
  });
}
