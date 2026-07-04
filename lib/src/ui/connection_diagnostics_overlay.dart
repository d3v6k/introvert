import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';

/// Floating glassmorphic overlay that shows live connection quality diagnostics.
///
/// Listens to Event Type 15 from the network stream and displays a sequential
/// checklist of transport pathways being tested, with animated status indicators.
class ConnectionDiagnosticsOverlay extends StatefulWidget {
  final String peerId;
  final IntrovertClient client;
  final VoidCallback onDismiss;

  const ConnectionDiagnosticsOverlay({
    super.key,
    required this.peerId,
    required this.client,
    required this.onDismiss,
  });

  @override
  State<ConnectionDiagnosticsOverlay> createState() =>
      _ConnectionDiagnosticsOverlayState();
}

/// The transport steps in sequential order for the checklist display.
enum _TransportStep {
  directP2P,
  relayedUdpQuic,
  relayedTcp,
  webSocketTunnel,
}

/// Status of each checklist item.
enum _StepStatus {
  pending,    // Not yet tested
  testing,    // Currently being tested (animated pulse)
  chosen,     // This transport was selected ✅
  skipped,    // Failed or skipped ❌
}

class _ConnectionDiagnosticsOverlayState
    extends State<ConnectionDiagnosticsOverlay>
    with TickerProviderStateMixin {

  StreamSubscription<NetworkEvent>? _networkSubscription;
  Timer? _autoDismissTimer;
  Timer? _pacingTimer;
  Timer? _safetyTimeoutTimer;

  // State for each step
  final Map<_TransportStep, _StepStatus> _stepStatuses = {
    _TransportStep.directP2P: _StepStatus.pending,
    _TransportStep.relayedUdpQuic: _StepStatus.pending,
    _TransportStep.relayedTcp: _StepStatus.pending,
    _TransportStep.webSocketTunnel: _StepStatus.pending,
  };

  // Current active step index for sequential pacing
  int _currentStepIndex = -1;

  // Final result
  bool _isSettled = false;
  String? _settledTransport;
  int? _settledRttMs;

  // Pulsing animation for the 🔍 icon
  late AnimationController _pulseController;
  late Animation<double> _pulseAnimation;

  // Slide-in animation
  late AnimationController _slideController;
  late Animation<Offset> _slideAnimation;

  @override
  void initState() {
    super.initState();

    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 800),
    )..repeat(reverse: true);
    _pulseAnimation = Tween<double>(begin: 0.3, end: 1.0).animate(
      CurvedAnimation(parent: _pulseController, curve: Curves.easeInOut),
    );

    _slideController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 400),
    );
    _slideAnimation = Tween<Offset>(
      begin: const Offset(0, -0.3),
      end: Offset.zero,
    ).animate(CurvedAnimation(parent: _slideController, curve: Curves.easeOutCubic));
    _slideController.forward();

    _startListening();
    _beginSequentialScan();

    // Safety fallback timeout in case FFI events are lost or delayed
    _safetyTimeoutTimer = Timer(const Duration(seconds: 10), () {
      if (mounted && !_isSettled) {
        _handleDiagnosticTimeoutFallback();
      }
    });
  }

  void _startListening() {
    _networkSubscription = widget.client.networkStream.listen((event) {
      if (event.type != 15) return;

      try {
        final jsonStr = utf8.decode(event.data);
        final data = json.decode(jsonStr) as Map<String, dynamic>;
        final peerId = data['peer_id'] as String? ?? '';
        final step = data['step'] as String? ?? '';
        final transport = data['transport'] as String? ?? '';
        final rttMs = data['rtt_ms'] as int? ?? 0;

        // Filter for this specific peer
        if (peerId.isNotEmpty && peerId != widget.peerId) return;

        _handleDiagnosticEvent(step, transport, rttMs);
      } catch (e) {
        debugPrint('⚠️ ConnectionDiagnostics: Error parsing event: $e');
      }
    });
  }

  void _handleDiagnosticEvent(String step, String transport, int rttMs) {
    if (!mounted) return;

    final transportStep = _mapTransport(transport);

    if (step == 'settled' || step == 'connected') {
      _safetyTimeoutTimer?.cancel();
      setState(() {
        // Mark the chosen transport
        if (transportStep != null) {
          _stepStatuses[transportStep] = _StepStatus.chosen;
        }

        // Mark all remaining pending/testing steps as skipped
        for (final entry in _stepStatuses.entries) {
          if (entry.value == _StepStatus.pending ||
              entry.value == _StepStatus.testing) {
            if (entry.key != transportStep) {
              _stepStatuses[entry.key] = _StepStatus.skipped;
            }
          }
        }

        _isSettled = true;
        _settledTransport = transport;
        _settledRttMs = rttMs;
        _currentStepIndex = _TransportStep.values.length;
      });

      _pacingTimer?.cancel();

      // Auto-dismiss after 2 seconds
      _autoDismissTimer?.cancel();
      _autoDismissTimer = Timer(const Duration(seconds: 2), () {
        if (mounted) {
          _slideController.reverse().then((_) {
            if (mounted) widget.onDismiss();
          });
        }
      });
    } else if (step == 'timeout' || step == 'error') {
      _safetyTimeoutTimer?.cancel();
      setState(() {
        if (transportStep != null) {
          _stepStatuses[transportStep] = _StepStatus.skipped;
        }
        // Mark all remaining as skipped
        for (final entry in _stepStatuses.entries) {
          if (entry.value == _StepStatus.pending ||
              entry.value == _StepStatus.testing) {
            _stepStatuses[entry.key] = _StepStatus.skipped;
          }
        }
        _isSettled = true;
        _settledTransport = 'Failed';
        _settledRttMs = null;
        _currentStepIndex = _TransportStep.values.length;
      });

      _pacingTimer?.cancel();
      _autoDismissTimer?.cancel();
      _autoDismissTimer = Timer(const Duration(seconds: 3), () {
        if (mounted) {
          _slideController.reverse().then((_) {
            if (mounted) widget.onDismiss();
          });
        }
      });
    }
  }

  /// Begin the sequential visual pacing (300ms per step minimum).
  void _beginSequentialScan() {
    _advanceStep();
  }

  void _advanceStep() {
    if (!mounted || _isSettled) return;

    _currentStepIndex++;
    if (_currentStepIndex >= _TransportStep.values.length) return;

    final currentStep = _TransportStep.values[_currentStepIndex];

    setState(() {
      // Mark current step as testing
      _stepStatuses[currentStep] = _StepStatus.testing;
    });

    // Schedule next step advancement after minimum pacing
    _pacingTimer = Timer(const Duration(milliseconds: 350), () {
      if (!mounted || _isSettled) return;

      // If no event settled this step, leave it testing and advance
      // (the actual network event will update the status)
      _advanceStep();
    });
  }

  _TransportStep? _mapTransport(String transport) {
    switch (transport) {
      case 'Direct P2P':
        return _TransportStep.directP2P;
      case 'Relayed UDP/QUIC (Port 443)':
        return _TransportStep.relayedUdpQuic;
      case 'Relayed TCP (Port 443)':
        return _TransportStep.relayedTcp;
      case 'WebSocket Tunnel (Port 80)':
        return _TransportStep.webSocketTunnel;
      default:
        return null;
    }
  }

  String _stepLabel(_TransportStep step) {
    switch (step) {
      case _TransportStep.directP2P:
        return 'Scanning Direct P2P pathways...';
      case _TransportStep.relayedUdpQuic:
        return 'Testing Relayed UDP/QUIC (Port 443)...';
      case _TransportStep.relayedTcp:
        return 'Testing Relayed TCP (Port 443)...';
      case _TransportStep.webSocketTunnel:
        return 'Testing Secure WebSocket Tunnel (Port 80)...';
    }
  }

  @override
  void dispose() {
    _networkSubscription?.cancel();
    _autoDismissTimer?.cancel();
    _pacingTimer?.cancel();
    _safetyTimeoutTimer?.cancel();
    _pulseController.dispose();
    _slideController.dispose();
    super.dispose();
  }

  void _handleDiagnosticTimeoutFallback() {
    if (!mounted || _isSettled) return;

    setState(() {
      // Mark all pending or testing steps as skipped/failed
      for (final entry in _stepStatuses.entries) {
        if (entry.value == _StepStatus.pending ||
            entry.value == _StepStatus.testing) {
          _stepStatuses[entry.key] = _StepStatus.skipped;
        }
      }
      _isSettled = true;
      _settledTransport = 'Timeout (No Response)';
      _settledRttMs = null;
      _currentStepIndex = _TransportStep.values.length;
    });

    _pacingTimer?.cancel();
    _autoDismissTimer?.cancel();
    _autoDismissTimer = Timer(const Duration(seconds: 3), () {
      if (mounted) {
        _slideController.reverse().then((_) {
          if (mounted) widget.onDismiss();
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return SlideTransition(
      position: _slideAnimation,
      child: Padding(
        padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(20),
          child: BackdropFilter(
            filter: ui.ImageFilter.blur(sigmaX: 24, sigmaY: 24),
            child: Container(
              padding: EdgeInsets.all(20),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(20),
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [
                    AppTheme.current.bg.withValues(alpha: 0.85),
                    const Color(0xFF0F1520).withValues(alpha: 0.75),
                  ],
                ),
                border: Border.all(
                  color: _isSettled
                      ? AppTheme.current.accent.withValues(alpha: 0.3)
                      : AppTheme.current.accent.withValues(alpha: 0.2),
                  width: 1,
                ),
                boxShadow: [
                  BoxShadow(
                    color: (_isSettled ? AppTheme.current.accent : AppTheme.current.accent)
                        .withValues(alpha: 0.08),
                    blurRadius: 30,
                    spreadRadius: 2,
                  ),
                ],
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Title row
                  Row(
                    children: [
                      AnimatedSwitcher(
                        duration: const Duration(milliseconds: 300),
                        child: Icon(
                          _isSettled
                              ? Icons.check_circle_rounded
                              : Icons.wifi_find_rounded,
                          key: ValueKey(_isSettled),
                          color: _isSettled
                              ? AppTheme.current.accent
                              : AppTheme.current.accent,
                          size: 20,
                        ),
                      ),
                      SizedBox(width: 10),
                      AnimatedSwitcher(
                        duration: const Duration(milliseconds: 300),
                        child: Text(
                          _isSettled
                              ? 'Connection Settled'
                              : 'Optimizing Connection...',
                          key: ValueKey(_isSettled),
                          style: TextStyle(
                            fontSize: 14,
                            fontWeight: FontWeight.bold,
                            color: _isSettled
                                ? AppTheme.current.accent
                                : AppTheme.current.text,
                            letterSpacing: 0.5,
                          ),
                        ),
                      ),
                    ],
                  ),
                  SizedBox(height: 16),

                  // Checklist items
                  ..._TransportStep.values.map((step) =>
                      _buildChecklistItem(step)),

                  // Settled result line
                  if (_isSettled && _settledTransport != null) ...[
                    SizedBox(height: 12),
                    Container(
                      padding: EdgeInsets.symmetric(
                          horizontal: 12, vertical: 8),
                      decoration: BoxDecoration(
                        color: (_settledTransport == 'Failed'
                                ? Colors.redAccent
                                : AppTheme.current.accent)
                            .withValues(alpha: 0.1),
                        borderRadius: BorderRadius.circular(10),
                        border: Border.all(
                          color: (_settledTransport == 'Failed'
                                  ? Colors.redAccent
                                  : AppTheme.current.accent)
                              .withValues(alpha: 0.2),
                        ),
                      ),
                      child: Row(
                        children: [
                          Icon(
                            _settledTransport == 'Failed'
                                ? Icons.error_outline_rounded
                                : Icons.speed_rounded,
                            size: 16,
                            color: _settledTransport == 'Failed'
                                ? Colors.redAccent
                                : AppTheme.current.accent,
                          ),
                          SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              _settledTransport == 'Failed'
                                  ? 'Connection failed — peer unreachable'
                                  : 'Connection settled: $_settledTransport${_settledRttMs != null ? ' (${_settledRttMs}ms)' : ''}',
                              style: TextStyle(
                                fontSize: 11,
                                fontWeight: FontWeight.w600,
                                color: _settledTransport == 'Failed'
                                    ? Colors.redAccent
                                    : AppTheme.current.accent,
                                fontFamily: 'monospace',
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildChecklistItem(_TransportStep step) {
    final status = _stepStatuses[step]!;
    final stepIndex = _TransportStep.values.indexOf(step);
    final isVisible = stepIndex <= _currentStepIndex;

    return AnimatedOpacity(
      opacity: isVisible ? 1.0 : 0.0,
      duration: const Duration(milliseconds: 250),
      child: Padding(
        padding: EdgeInsets.symmetric(vertical: 4),
        child: Row(
          children: [
            SizedBox(
              width: 22,
              height: 22,
              child: Center(child: _buildStatusIcon(status)),
            ),
            SizedBox(width: 10),
            Expanded(
              child: Text(
                _stepLabel(step),
                style: TextStyle(
                  fontSize: 12,
                  color: status == _StepStatus.chosen
                      ? AppTheme.current.accent
                      : status == _StepStatus.skipped
                          ? AppTheme.current.mutedText.withValues(alpha: 0.5)
                          : AppTheme.current.text.withValues(alpha: 0.7),
                  fontWeight: status == _StepStatus.chosen
                      ? FontWeight.bold
                      : FontWeight.normal,
                  decoration: status == _StepStatus.skipped
                      ? TextDecoration.lineThrough
                      : null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatusIcon(_StepStatus status) {
    switch (status) {
      case _StepStatus.pending:
        return Text('⏳', style: TextStyle(fontSize: 13));
      case _StepStatus.testing:
        return AnimatedBuilder(
          animation: _pulseAnimation,
          builder: (context, child) => Opacity(
            opacity: _pulseAnimation.value,
            child: Text('🔍', style: TextStyle(fontSize: 13)),
          ),
        );
      case _StepStatus.chosen:
        return Text('✅', style: TextStyle(fontSize: 13));
      case _StepStatus.skipped:
        return Text('❌', style: TextStyle(fontSize: 13));
    }
  }
}

/// Shows the connection diagnostics overlay as a top-positioned widget
/// within an existing Stack or Overlay.
void showConnectionDiagnostics({
  required BuildContext context,
  required String peerId,
  required IntrovertClient client,
}) {
  late OverlayEntry overlayEntry;

  overlayEntry = OverlayEntry(
    builder: (context) => Positioned(
      top: MediaQuery.of(context).padding.top + kToolbarHeight + 8,
      left: 0,
      right: 0,
      child: Material(
        color: Colors.transparent,
        child: ConnectionDiagnosticsOverlay(
          peerId: peerId,
          client: client,
          onDismiss: () {
            overlayEntry.remove();
          },
        ),
      ),
    ),
  );

  Overlay.of(context).insert(overlayEntry);
}
