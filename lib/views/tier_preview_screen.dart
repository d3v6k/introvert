import 'package:flutter/material.dart';
import '../blueprint_ui.dart';
import '../theme/app_theme.dart';

class TierPreviewScreen extends StatelessWidget {
  const TierPreviewScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        title: Text('PRESTIGE TIERS', style: TextStyle(fontFamily: 'monospace', letterSpacing: 2)),
        backgroundColor: Colors.transparent,
        elevation: 0,
      ),
      body: SingleChildScrollView(
        padding: EdgeInsets.all(24),
        child: Column(
          children: [
            Text(
              'Avatar Tier Preview',
              style: TextStyle(color: AppTheme.current.text, fontSize: 18, fontWeight: FontWeight.bold),
            ),
            SizedBox(height: 8),
            Text(
              'Your tier badge is visible to ALL contacts.\nMetallic ring animates for premium tiers.',
              textAlign: TextAlign.center,
              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12),
            ),
            SizedBox(height: 32),
            _buildTierRow('CITIZEN', '< 100,000 INTR', SovereignAvatar(radius: 30, prestigeTier: 0, initials: 'C')),
            SizedBox(height: 24),
            _buildTierRow('SENTINEL', '≥ 100,000 INTR', SovereignAvatar(radius: 30, prestigeTier: 1, initials: 'S')),
            SizedBox(height: 24),
            _buildTierRow('SILVER', '≥ 250,000 INTR', SovereignAvatar(radius: 30, prestigeTier: 2, initials: 'S')),
            SizedBox(height: 24),
            _buildTierRow('GOLD', '≥ 500,000 INTR', SovereignAvatar(radius: 30, prestigeTier: 3, initials: 'G')),
            SizedBox(height: 24),
            _buildTierRow('PLATINUM', '≥ 1,000,000 INTR', SovereignAvatar(radius: 30, prestigeTier: 4, initials: 'P')),
            SizedBox(height: 32),
            Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
            SizedBox(height: 16),
            Text(
              'Activity Tiers (Validated)',
              style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold),
            ),
            SizedBox(height: 8),
            Text(
              'Drops back if activity lapses.\nCatalyst: 30-day referral window. Pulsar: 7-day activity window.',
              textAlign: TextAlign.center,
              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11),
            ),
            SizedBox(height: 24),
            _buildTierRow('CATALYST', 'Growth champion', SovereignAvatar(radius: 30, prestigeTier: 5, initials: 'C')),
            SizedBox(height: 24),
            _buildTierRow('PULSAR', 'Super active', SovereignAvatar(radius: 30, prestigeTier: 6, initials: 'P')),
            SizedBox(height: 32),
            Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
            SizedBox(height: 16),
            Text(
              'How Contacts See You in Chat',
              style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold),
            ),
            SizedBox(height: 8),
            Text(
              'Your tier badge appears on every message you send.\nThis is what your contacts see in 1:1 and group chats.',
              textAlign: TextAlign.center,
              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11),
            ),
            SizedBox(height: 24),
            _buildChatSimulation(),
          ],
        ),
      ),
    );
  }

  Widget _buildTierRow(String name, String requirement, Widget avatar) {
    return Row(
      children: [
        avatar,
        SizedBox(width: 16),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(name, style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
              SizedBox(height: 2),
              Text(requirement, style: TextStyle(color: AppTheme.current.accent, fontSize: 12)),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildChatSimulation() {
    final tiers = [
      ('Citizen', 0),
      ('Sentinel', 1),
      ('Silver', 2),
      ('Gold', 3),
      ('Platinum', 4),
      ('Catalyst', 5),
      ('Pulsar', 6),
    ];

    return Container(
      padding: EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: AppTheme.current.surface.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
      ),
      child: Column(
        children: [
          // Simulated incoming messages from different tiers
          ...tiers.map((t) {
            final (name, tier) = t;
            return Padding(
              padding: EdgeInsets.symmetric(vertical: 6),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  SovereignAvatar(radius: 28, prestigeTier: tier, initials: name[0]),
                  SizedBox(width: 8),
                  Flexible(
                    child: Container(
                      padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                      decoration: BoxDecoration(
                        color: AppTheme.current.text.withValues(alpha: 0.05),
                        borderRadius: BorderRadius.circular(12).copyWith(bottomLeft: Radius.circular(0)),
                        border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(name.toUpperCase(), style: TextStyle(color: AppTheme.current.accent, fontSize: 9, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
                          SizedBox(height: 2),
                          Text('Hello from $name tier', style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            );
          }),
          SizedBox(height: 12),
          // Simulated outgoing message (Platinum user)
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Spacer(flex: 2),
              Flexible(
                flex: 5,
                child: Container(
                  padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    color: AppTheme.current.accent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(12).copyWith(bottomRight: Radius.circular(0)),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                  ),
                  child: Text('This is how your Platinum message looks to contacts', style: TextStyle(color: AppTheme.current.text, fontSize: 13)),
                ),
              ),
              SizedBox(width: 8),
              SovereignAvatar(radius: 28, prestigeTier: 4, initials: 'P'),
            ],
          ),
        ],
      ),
    );
  }
}
