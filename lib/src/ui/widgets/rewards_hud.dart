import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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

class NodeDashboard extends StatefulWidget {
  final Map<String, dynamic> economyStats;

  const NodeDashboard({
    super.key,
    required this.economyStats,
  });

  @override
  State<NodeDashboard> createState() => _NodeDashboardState();
}

class _NodeDashboardState extends State<NodeDashboard> {
  bool _noticeExpanded = false;

  @override
  Widget build(BuildContext context) {
    final intrBalance = (widget.economyStats['intr_balance'] ?? 0) / 1000000000.0;
    final solBalance = (widget.economyStats['sol_balance'] ?? 0) / 1000000000.0;
    final usdcBalance = (widget.economyStats['usdc_balance'] ?? 0) / 1000000.0;
    final pendingPoints = (widget.economyStats['pending_rewards'] ?? 0) / 1000000000.0;
    final solAddress = widget.economyStats['sol_address'] ?? 'Connecting...';

    return Padding(
      padding: EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Header
          Row(
            children: [
              Icon(Icons.account_balance_wallet_rounded, color: AppTheme.current.accent, size: 20),
              SizedBox(width: 8),
              Expanded(
                child: Text(
                  "SOVEREIGN WALLET",
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.bold,
                    color: AppTheme.current.accent,
                    letterSpacing: 1.1,
                  ),
                ),
              ),
              Container(
                padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: Colors.purple.withValues(alpha: 0.1),
                  borderRadius: BorderRadius.circular(4),
                  border: Border.all(color: Colors.purpleAccent.withValues(alpha: 0.2)),
                ),
                child: Text(
                  "SOLANA NETWORK",
                  style: TextStyle(
                    color: Colors.purpleAccent,
                    fontSize: 8,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
            ],
          ),
          Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1), height: 24),

          // Node Identifier
          Material(
            color: Colors.transparent,
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              title: Text('Solana Wallet Address', style: TextStyle(fontSize: 13, color: AppTheme.current.text.withValues(alpha: 0.7))),
              subtitle: Text(
                solAddress,
                style: TextStyle(fontFamily: 'monospace', fontSize: 11, color: AppTheme.current.text.withValues(alpha: 0.5)),
                overflow: TextOverflow.ellipsis,
              ),
              trailing: IconButton(
                icon: Icon(Icons.copy, size: 18, color: AppTheme.current.mutedText),
                onPressed: () {
                  if (solAddress != 'Connecting...') {
                    Clipboard.setData(ClipboardData(text: solAddress));
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text('Wallet address copied to clipboard')),
                    );
                  }
                },
              ),
            ),
          ),
          Text(
            "Your wallet is derived on the Solana network from your Sovereign master seed phrase, keeping your keys unified yet cryptographically isolated for safety.",
            style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 10.5, height: 1.3),
          ),
          SizedBox(height: 16),

          // Asset Balances
          Text(
            "ASSET BALANCES",
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: AppTheme.current.text.withValues(alpha: 0.7),
              letterSpacing: 1.0,
            ),
          ),
          SizedBox(height: 8),
          _buildBalanceRow('INTR', intrBalance, 4),
          _buildBalanceRow('SOL', solBalance, 4),
          _buildBalanceRow('USDC', usdcBalance, 2),
          SizedBox(height: 12),

          // Points Earned
          _buildStatRow("Points Earned (This Cycle)", "${pendingPoints.toStringAsFixed(1)} pts"),
          SizedBox(height: 16),

          // Sovereign Distribution Notice — expandable
          _buildDistributionNotice(),
        ],
      ),
    );
  }

  Widget _buildBalanceRow(String symbol, double amount, int decimals) {
    return Padding(
      padding: EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(symbol, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)),
          Flexible(
            child: Text(
              "${amount.toStringAsFixed(decimals)} $symbol",
              style: TextStyle(
                color: AppTheme.current.text,
                fontWeight: FontWeight.bold,
                fontFamily: 'monospace',
                fontSize: 13,
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
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
          Flexible(
            child: Text(value, style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, fontFamily: 'monospace'), overflow: TextOverflow.ellipsis),
          ),
        ],
      ),
    );
  }

  Widget _buildDistributionNotice() {
    return GestureDetector(
      onTap: () => setState(() => _noticeExpanded = !_noticeExpanded),
      child: Container(
        padding: EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: AppTheme.current.accent.withValues(alpha: 0.06),
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.15)),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.info_outline, color: AppTheme.current.accent, size: 16),
                SizedBox(width: 8),
                Expanded(
                  child: Text(
                    "SOVEREIGN DISTRIBUTION NOTICE",
                    style: TextStyle(
                      color: AppTheme.current.accent,
                      fontWeight: FontWeight.bold,
                      fontSize: 10,
                      letterSpacing: 1,
                    ),
                  ),
                ),
                Icon(
                  _noticeExpanded ? Icons.expand_less : Icons.expand_more,
                  color: AppTheme.current.mutedText,
                  size: 18,
                ),
              ],
            ),
            if (_noticeExpanded) ...[
              SizedBox(height: 8),
              Text(
                "Introvert is strictly an open-source, non-custodial communication tool. It is not a financial platform, money transmitter, or cryptocurrency wallet.",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 11, height: 1.4),
              ),
              SizedBox(height: 8),
              Text(
                "This dashboard displays a local, read-only ledger of your peer-to-peer mesh participation points. This application code handles the collection of activity parameters entirely locally on your device. At the close of each cycle, these raw activity logs are securely transmitted to the decentralized Root Bootstrap Node (RBN) network, where final participation allocations are calculated independently against the ecosystem's macro-emission schedule.",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 11, height: 1.4),
              ),
              SizedBox(height: 8),
              Text(
                "The Introvert client app contains no financial execution pipelines, holds zero digital assets, and completely lacks the programmatic capacity to sign blockchain transactions or process monetary transfers. Viewing, holding, or interacting with your allocated \$INTR tokens must be performed entirely outside this software using independent, third-party Solana wallet interfaces (such as Phantom, Solflare, or Backpack) at your own sole risk and discretion.",
                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 11, height: 1.4),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
