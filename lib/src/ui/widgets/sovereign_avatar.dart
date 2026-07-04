import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../../../theme/app_theme.dart';

enum PrestigeTier {
  citizen,   // < 500 INTR
  sentinel,  // >= 500 INTR — Edge Relay Node
  silver,    // >= 10,000 INTR
  gold,      // >= 25,000 INTR
  platinum,  // >= 100,000 INTR
  catalyst,  // Growth/onboarding champion (validated)
  pulsar,    // Super active communicator (validated)
}

class SovereignAvatar extends StatefulWidget {
  final double radius;
  final ImageProvider? avatar;
  final String? initials;
  final double balance;
  final bool isSuperActive;
  final bool isGrowthCatalyst;
  final int? lastActivityTimestamp;
  final int? lastReferralTimestamp;
  final int? prestigeTier; // Direct override: 0=Citizen, 1=Sentinel, 2=Silver, 3=Gold, 4=Platinum, 5=Catalyst, 6=Pulsar

  const SovereignAvatar({
    super.key,
    required this.radius,
    this.avatar,
    this.initials,
    this.balance = 0,
    this.isSuperActive = false,
    this.isGrowthCatalyst = false,
    this.lastActivityTimestamp,
    this.lastReferralTimestamp,
    this.prestigeTier,
  });

  @override
  State<SovereignAvatar> createState() => _SovereignAvatarState();
}

class _SovereignAvatarState extends State<SovereignAvatar> with SingleTickerProviderStateMixin {
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 4),
    );
    // Only start animation if user has a non-citizen tier
    if (widget.prestigeTier != null && widget.prestigeTier! > 0 ||
        widget.balance > 0 || widget.isSuperActive || widget.isGrowthCatalyst) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  PrestigeTier _getTier() {
    if (widget.prestigeTier != null) {
      final t = widget.prestigeTier!.clamp(0, 6);
      return PrestigeTier.values[t];
    }
    if (widget.isGrowthCatalyst && _isCatalystValid()) return PrestigeTier.catalyst;
    if (widget.isSuperActive && _isPulsarValid()) return PrestigeTier.pulsar;
    if (widget.balance >= 1000000) return PrestigeTier.platinum;
    if (widget.balance >= 500000) return PrestigeTier.gold;
    if (widget.balance >= 250000) return PrestigeTier.silver;
    if (widget.balance >= 100000) return PrestigeTier.sentinel;
    return PrestigeTier.citizen;
  }

  bool _isCatalystValid() {
    if (widget.lastReferralTimestamp == null) return true;
    final elapsed = DateTime.now().millisecondsSinceEpoch - widget.lastReferralTimestamp!;
    return elapsed < const Duration(days: 30).inMilliseconds;
  }

  bool _isPulsarValid() {
    if (widget.lastActivityTimestamp == null) return true;
    final elapsed = DateTime.now().millisecondsSinceEpoch - widget.lastActivityTimestamp!;
    return elapsed < const Duration(days: 7).inMilliseconds;
  }

  double _tierScale(PrestigeTier tier) {
    switch (tier) {
      case PrestigeTier.citizen: return 1.0;
      case PrestigeTier.sentinel: return 1.15;
      case PrestigeTier.silver: return 1.30;
      case PrestigeTier.gold: return 1.50;
      case PrestigeTier.platinum: return 1.75;
      case PrestigeTier.catalyst: return 1.25;
      case PrestigeTier.pulsar: return 1.25;
    }
  }

  @override
  Widget build(BuildContext context) {
    final tier = _getTier();
    final scale = _tierScale(tier);
    final effectiveRadius = widget.radius * scale;
    final ringWidth = math.max(2.0, widget.radius * 0.12);
    final stackSize = (effectiveRadius + ringWidth) * 2;

    return SizedBox(
      width: stackSize,
      height: stackSize,
      child: AnimatedBuilder(
        animation: _controller,
        builder: (context, child) {
          return Stack(
            alignment: Alignment.center,
            children: [
              if (tier != PrestigeTier.citizen)
                _buildMetallicRing(tier, effectiveRadius, ringWidth, _controller.value),
              CircleAvatar(
                radius: effectiveRadius,
                backgroundColor: AppTheme.current.surface,
                backgroundImage: widget.avatar ?? const AssetImage('assets/images/default_avatar.png'),
                child: widget.avatar == null && widget.initials != null && widget.initials!.isNotEmpty
                    ? Text(
                        widget.initials!,
                        style: TextStyle(
                          color: AppTheme.current.accent,
                          fontSize: effectiveRadius * 0.8,
                          fontWeight: FontWeight.bold,
                        ),
                      )
                    : null,
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildMetallicRing(PrestigeTier tier, double radius, double width, double rotation) {
    final size = (radius + width) * 2;
    final colors = _metallicColors(tier);

    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: SweepGradient(
          colors: colors,
          transform: GradientRotation(rotation * 2 * math.pi),
        ),
        boxShadow: [
          BoxShadow(
            color: _tierAccentColor(tier).withValues(alpha: 0.6),
            blurRadius: 8,
            spreadRadius: 1,
          ),
        ],
      ),
    );
  }

  List<Color> _metallicColors(PrestigeTier tier) {
    switch (tier) {
      case PrestigeTier.sentinel:
        return [
          AppTheme.current.accent,
          Colors.cyan.shade700,
          AppTheme.current.accent.withValues(alpha: 0.6),
          Colors.cyan.shade300,
          AppTheme.current.accent,
        ];
      case PrestigeTier.silver:
        return [
          Colors.grey.shade700,
          Colors.grey.shade400,
          Colors.grey.shade200,
          Colors.grey.shade500,
          Colors.grey.shade700,
        ];
      case PrestigeTier.gold:
        return [
          Colors.amber.shade300,
          Colors.orange.shade700,
          Colors.yellow.shade600,
          Colors.amber.shade800,
          Colors.amber.shade300,
        ];
      case PrestigeTier.platinum:
        return [
          Colors.white,
          Colors.cyan.shade100,
          Colors.blue.shade200,
          Colors.white,
          Colors.cyan.shade50,
        ];
      case PrestigeTier.catalyst:
        return [
          Colors.purpleAccent.shade100,
          Colors.deepPurple.shade700,
          Colors.purpleAccent,
          Colors.deepPurple.shade400,
          Colors.purpleAccent.shade100,
        ];
      case PrestigeTier.pulsar:
        return [
          Colors.redAccent.shade100,
          Colors.red.shade800,
          Colors.redAccent,
          Colors.red.shade500,
          Colors.redAccent.shade100,
        ];
      default:
        return [Colors.transparent, Colors.transparent];
    }
  }

  Color _tierAccentColor(PrestigeTier tier) {
    switch (tier) {
      case PrestigeTier.sentinel: return AppTheme.current.accent;
      case PrestigeTier.silver: return Colors.grey.shade400;
      case PrestigeTier.gold: return Colors.amber;
      case PrestigeTier.platinum: return Colors.cyan.shade200;
      case PrestigeTier.catalyst: return Colors.purpleAccent;
      case PrestigeTier.pulsar: return Colors.redAccent;
      default: return Colors.transparent;
    }
  }
}
