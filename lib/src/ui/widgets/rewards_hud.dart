import 'package:flutter/material.dart';
import '../../../theme/app_theme.dart';

class RewardsHUD extends StatelessWidget {
  final int relayedBytes;
  final double solRewards;

  const RewardsHUD({
    super.key,
    required this.relayedBytes,
    required this.solRewards,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: BoxDecoration(
        color: AppTheme.current.bg.withValues(alpha: 0.85),
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.bolt, color: AppTheme.current.accent, size: 16),
          SizedBox(width: 8),
          Text(
            "${(relayedBytes / 1024 / 1024).toStringAsFixed(2)} MB",
            style: TextStyle(
              color: AppTheme.current.text,
              fontFamily: 'monospace',
              fontSize: 12,
            ),
          ),
          SizedBox(width: 12),
          VerticalDivider(color: AppTheme.current.mutedText.withValues(alpha: 0.5), width: 1, thickness: 1),
          SizedBox(width: 12),
          Text(
            "${solRewards.toStringAsFixed(6)} INTR",
            style: TextStyle(
              color: AppTheme.current.accent,
              fontFamily: 'monospace',
              fontSize: 12,
              fontWeight: FontWeight.bold,
            ),
          ),
        ],
      ),
    );
  }
}

class SovereignEarnings extends StatelessWidget {
  final Map<String, dynamic> economyStats;
  final VoidCallback onClaim;

  const SovereignEarnings({
    super.key,
    required this.economyStats,
    required this.onClaim,
  });

  @override
  Widget build(BuildContext context) {
    final balance = (economyStats['intr_balance'] ?? 0) / 1000000000.0;
    final pending = (economyStats['pending_rewards'] ?? 0) / 1000000000.0;
    final totalRelayed = (economyStats['total_relayed'] ?? 0) / 1024 / 1024;
    final tokenName = economyStats['token_name'] ?? "INTR";

    return Card(
      color: AppTheme.current.surface,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
      ),
      child: Padding(
        padding: EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  "SOVEREIGN EARNINGS",
                  style: TextStyle(
                    color: AppTheme.current.accent,
                    fontWeight: FontWeight.bold,
                    letterSpacing: 1.2,
                  ),
                ),
                Icon(Icons.account_balance_wallet_outlined, color: AppTheme.current.accent.withValues(alpha: 0.5)),
              ],
            ),
            SizedBox(height: 16),
            _buildStatRow("$tokenName Balance", "${balance.toStringAsFixed(4)} $tokenName"),
            _buildStatRow("Pending Activity Yield", "${pending.toStringAsFixed(4)} $tokenName"),
            _buildStatRow("Lifetime Relayed", "${totalRelayed.toStringAsFixed(2)} MB"),
            SizedBox(height: 16),
            SizedBox(
              width: double.infinity,
              child: ElevatedButton(
                onPressed: pending > 0.0 ? onClaim : null,
                style: ElevatedButton.styleFrom(
                  backgroundColor: AppTheme.current.accent,
                  foregroundColor: Colors.black,
                  disabledBackgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                ),
                child: Text("CLAIM REWARDS"),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatRow(String label, String value) {
    return Padding(
      padding: EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)),
          Text(value, style: TextStyle(color: AppTheme.current.text, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
        ],
      ),
    );
  }
}
