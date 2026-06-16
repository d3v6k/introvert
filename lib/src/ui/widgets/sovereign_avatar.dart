import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../../../theme/app_theme.dart';

enum PrestigeTier {
  citizen,
  squire,
  silver,
  gold,
  platinum,
  catalyst, // High onboarding/referrals
  pulsar,   // Super active communication
}

class SovereignAvatar extends StatefulWidget {
  final double radius;
  final ImageProvider? avatar;
  final double balance;
  final bool isSuperActive;
  final bool isGrowthCatalyst;

  const SovereignAvatar({
    super.key,
    required this.radius,
    this.avatar,
    this.balance = 0,
    this.isSuperActive = false,
    this.isGrowthCatalyst = false,
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
      duration: const Duration(seconds: 3),
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  PrestigeTier _getTier() {
    if (widget.isGrowthCatalyst) return PrestigeTier.catalyst;
    if (widget.isSuperActive) return PrestigeTier.pulsar;
    if (widget.balance >= 500000) return PrestigeTier.platinum;
    if (widget.balance >= 50000) return PrestigeTier.gold;
    if (widget.balance >= 5000) return PrestigeTier.silver;
    if (widget.balance >= 500) return PrestigeTier.squire;
    return PrestigeTier.citizen;
  }

  @override
  Widget build(BuildContext context) {
    final tier = _getTier();
    
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, child) {
        return Stack(
          alignment: Alignment.center,
          children: [
            if (tier != PrestigeTier.citizen)
              Container(
                width: (widget.radius * 2) + 8,
                height: (widget.radius * 2) + 8,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: _getTierGradient(tier, _controller.value),
                  boxShadow: [
                    BoxShadow(
                      color: _getTierColor(tier).withValues(alpha: 0.5),
                      blurRadius: 10,
                      spreadRadius: 2,
                    ),
                  ],
                ),
              ),
            CircleAvatar(
              radius: widget.radius,
              backgroundColor: Colors.black,
              backgroundImage: widget.avatar,
              child: widget.avatar == null
                  ? Icon(Icons.person, size: widget.radius, color: AppTheme.current.mutedText.withValues(alpha: 0.5))
                  : null,
            ),
          ],
        );
      },
    );
  }

  Color _getTierColor(PrestigeTier tier) {
    switch (tier) {
      case PrestigeTier.squire: return AppTheme.current.accent;
      case PrestigeTier.silver: return Colors.grey[300]!;
      case PrestigeTier.gold: return Colors.amber;
      case PrestigeTier.platinum: return AppTheme.current.text;
      case PrestigeTier.catalyst: return Colors.purpleAccent;
      case PrestigeTier.pulsar: return Colors.redAccent;
      default: return Colors.transparent;
    }
  }

  Gradient _getTierGradient(PrestigeTier tier, double rotation) {
    final colors = <Color>[];
    switch (tier) {
      case PrestigeTier.squire:
        colors.addAll([AppTheme.current.accent, Colors.cyan.shade900, AppTheme.current.accent]);
        break;
      case PrestigeTier.silver:
        colors.addAll([AppTheme.current.text, Colors.grey, AppTheme.current.text]);
        break;
      case PrestigeTier.gold:
        colors.addAll([Colors.amber, Colors.orange.shade900, Colors.amber]);
        break;
      case PrestigeTier.platinum:
        colors.addAll([AppTheme.current.text, Colors.blue.shade100, AppTheme.current.text]);
        break;
      case PrestigeTier.catalyst:
        colors.addAll([Colors.purpleAccent, Colors.deepPurple.shade900, Colors.purpleAccent]);
        break;
      case PrestigeTier.pulsar:
        colors.addAll([Colors.redAccent, Colors.red.shade900, Colors.redAccent]);
        break;
      default:
        return const RadialGradient(colors: [Colors.transparent, Colors.transparent]);
    }

    return SweepGradient(
      colors: colors,
      transform: GradientRotation(rotation * 2 * math.pi),
    );
  }
}
