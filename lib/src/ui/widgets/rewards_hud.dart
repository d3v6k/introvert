import 'package:flutter/material.dart';

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
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: BoxDecoration(
        color: Colors.black54,
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: Colors.cyanAccent.withOpacity(0.3)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.bolt, color: Colors.cyanAccent, size: 16),
          const SizedBox(width: 8),
          Text(
            "${(relayedBytes / 1024 / 1024).toStringAsFixed(2)} MB",
            style: const TextStyle(
              color: Colors.white,
              fontFamily: 'monospace',
              fontSize: 12,
            ),
          ),
          const SizedBox(width: 12),
          const VerticalDivider(color: Colors.white24, width: 1, thickness: 1),
          const SizedBox(width: 12),
          Text(
            "${solRewards.toStringAsFixed(6)} INTR",
            style: const TextStyle(
              color: Colors.greenAccent,
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
    final pending = (economyStats['pending_rewards'] ?? 0) / 1024 / 1024;
    final totalRelayed = (economyStats['total_relayed'] ?? 0) / 1024 / 1024;
    final address = economyStats['sol_address'] ?? "Unknown";
    final tokenName = economyStats['token_name'] ?? "INTR";

    return Card(
      color: Colors.grey[900],
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Colors.cyanAccent.withOpacity(0.2)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text(
                  "SOVEREIGN EARNINGS",
                  style: TextStyle(
                    color: Colors.cyanAccent,
                    fontWeight: FontWeight.bold,
                    letterSpacing: 1.2,
                  ),
                ),
                Icon(Icons.account_balance_wallet_outlined, color: Colors.cyanAccent.withOpacity(0.5)),
              ],
            ),
            const SizedBox(height: 16),
            _buildStatRow("$tokenName Balance", "${balance.toStringAsFixed(4)} $tokenName"),
            _buildStatRow("Pending Rewards", "${pending.toStringAsFixed(2)} MB"),
            _buildStatRow("Lifetime Relayed", "${totalRelayed.toStringAsFixed(2)} MB"),
            const Divider(color: Colors.white12, height: 24),
            Text(
              "WALLET: $address",
              style: TextStyle(color: Colors.white38, fontSize: 10, fontFamily: 'monospace'),
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 16),
            SizedBox(
              width: double.infinity,
              child: ElevatedButton(
                onPressed: pending >= 1.0 ? onClaim : null,
                style: ElevatedButton.styleFrom(
                  backgroundColor: Colors.cyanAccent,
                  foregroundColor: Colors.black,
                  disabledBackgroundColor: Colors.white12,
                ),
                child: const Text("CLAIM REWARDS"),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(color: Colors.white70, fontSize: 13)),
          Text(value, style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
        ],
      ),
    );
  }
}
