import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../../theme/app_theme.dart';
import '../../native/introvert_client.dart';

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
  bool _declaringPoints = false;
  String _declareStatus = '';
  Map<String, dynamic>? _referralStatus;
  StreamSubscription? _telemetryAckSub;
  StreamSubscription? _economySub;
  Map<String, dynamic> _liveEconomyStats = {};

  void _loadReferralStatus() {
    try {
      final status = IntrovertClient().getReferralStatus();
      if (mounted) {
        setState(() { _referralStatus = status; });
      }
    } catch (_) {}
  }

  @override
  void initState() {
    super.initState();
    _liveEconomyStats = widget.economyStats;
    _loadReferralStatus();
    // Listen to economy stream directly for real-time point updates
    _economySub = IntrovertClient().economyStream.listen((stats) {
      if (mounted) {
        setState(() {
          _liveEconomyStats = stats;
        });
      }
    });
  }

  @override
  void didUpdateWidget(NodeDashboard oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Update from parent if stream hasn't fired yet
    if (oldWidget.economyStats != widget.economyStats) {
      _liveEconomyStats = widget.economyStats;
    }
  }

  @override
  void dispose() {
    _telemetryAckSub?.cancel();
    _economySub?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final intrBalance = (_liveEconomyStats['intr_balance'] ?? 0) / 1000000000.0;
    final solBalance = (_liveEconomyStats['sol_balance'] ?? 0) / 1000000000.0;
    final usdcBalance = (_liveEconomyStats['usdc_balance'] ?? 0) / 1000000.0;
    final solAddress = _liveEconomyStats['sol_address'] ?? 'Connecting...';

    // Use real-time daily_earnings from DailyRewardEngine (updated every 30s)
    final dailyEarnings = _liveEconomyStats['daily_earnings'];
    final double socialPoints = (dailyEarnings is Map) ? ((dailyEarnings['social_points'] as num?)?.toDouble() ?? 0.0) : 0.0;
    final double infraPoints = (dailyEarnings is Map) ? ((dailyEarnings['infra_points'] as num?)?.toDouble() ?? 0.0) : 0.0;
    final double totalPoints = socialPoints + infraPoints;

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
          Row(
            children: [
              Text(
                "ASSET BALANCES",
                style: TextStyle(
                  fontSize: 10,
                  fontWeight: FontWeight.bold,
                  color: AppTheme.current.text.withValues(alpha: 0.7),
                  letterSpacing: 1.0,
                ),
              ),
              SizedBox(width: 8),
              Text(
                "(pull down to refresh)",
                style: TextStyle(
                  fontSize: 9,
                  color: AppTheme.current.mutedText.withValues(alpha: 0.5),
                  fontStyle: FontStyle.italic,
                ),
              ),
            ],
          ),
          SizedBox(height: 8),
          _buildBalanceRow('INTR', intrBalance, 4),
          _buildBalanceRow('SOL', solBalance, 4),
          _buildBalanceRow('USDC', usdcBalance, 2),
          SizedBox(height: 12),

          // Points Earned (real-time from DailyRewardEngine)
          Text(
            "POINTS EARNED (THIS CYCLE)",
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: AppTheme.current.text.withValues(alpha: 0.7),
              letterSpacing: 1.0,
            ),
          ),
          SizedBox(height: 8),
          _buildStatRow("Social Points", "${socialPoints.toStringAsFixed(1)} pts"),
          _buildStatRow("Infrastructure Points", "${infraPoints.toStringAsFixed(1)} pts"),
          _buildStatRow("Total Points", "${totalPoints.toStringAsFixed(1)} pts"),
          SizedBox(height: 12),

          // Declare Points to Mesh button
          SizedBox(
            width: double.infinity,
            child: OutlinedButton.icon(
              onPressed: _declaringPoints ? null : _declarePointsToMesh,
              icon: _declaringPoints
                  ? SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent))
                  : Icon(Icons.upload_outlined, size: 16, color: AppTheme.current.accent),
              label: Text(
                _declaringPoints ? 'Declaring...' : 'Declare Points to Mesh',
                style: TextStyle(fontSize: 12, color: AppTheme.current.accent),
              ),
              style: OutlinedButton.styleFrom(
                side: BorderSide(color: AppTheme.current.accent.withValues(alpha: 0.3)),
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
                padding: EdgeInsets.symmetric(vertical: 10),
              ),
            ),
          ),
          if (_declareStatus.isNotEmpty) ...[
            SizedBox(height: 6),
            Text(
              _declareStatus,
              style: TextStyle(
                fontSize: 10,
                color: _declareStatus.contains('Failed') || _declareStatus.contains('no RBN')
                    ? Colors.orangeAccent
                    : Colors.greenAccent,
              ),
              textAlign: TextAlign.center,
            ),
          ],
          // Referral Status
          if (_referralStatus != null && _referralStatus!['state'] != 'none') ...[
            _buildReferralStatusCard(_referralStatus!),
            SizedBox(height: 12),
          ],

          SizedBox(height: 16),

          // Sovereign Distribution Notice — expandable
          _buildDistributionNotice(),
        ],
      ),
    );
  }

  Future<void> _declarePointsToMesh() async {
    setState(() {
      _declaringPoints = true;
      _declareStatus = 'Sending telemetry to mesh...';
    });

    // Cancel any previous subscription
    _telemetryAckSub?.cancel();

    try {
      final success = IntrovertClient().sendManualTelemetry();
      if (!success) {
        setState(() {
          _declareStatus = 'Failed: no RBN connected. Connect to mesh first.';
          _declaringPoints = false;
        });
        return;
      }

      // Listen for TelemetryAck from RBN (Event 40)
      _telemetryAckSub = IntrovertClient().telemetryAckStream.listen((ack) {
        if (!mounted) return;
        final epochId = ack['epoch_id'] ?? 'unknown';
        setState(() {
          _declaringPoints = false;
          _declareStatus = 'RBN confirmed receipt for epoch $epochId. INTR distributed at epoch close.';
        });
        _telemetryAckSub?.cancel();
        // Clear status after 10 seconds
        Future.delayed(Duration(seconds: 10), () {
          if (mounted) setState(() => _declareStatus = '');
        });
      });

      // Timeout after 15 seconds if no ack received
      Future.delayed(Duration(seconds: 15), () {
        if (mounted && _declaringPoints) {
          setState(() {
            _declaringPoints = false;
            _declareStatus = 'No RBN confirmation received. Telemetry may not have reached the network.';
          });
          _telemetryAckSub?.cancel();
        }
      });
    } catch (e) {
      setState(() {
        _declaringPoints = false;
        _declareStatus = 'Failed: $e';
      });
    }
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

  Widget _buildReferralStatusCard(Map<String, dynamic> status) {
    final state = status['state'] as String? ?? 'none';
    final tierName = status['tier_name'] as String?;
    final activeUntil = status['active_until'] as String?;
    final newReferrals = status['todays_new_referrals'] as int? ?? 0;
    final bonusMult = status['todays_bonus_multiplier'] as double? ?? 1.0;

    String title;
    String subtitle;
    IconData icon;
    Color color;

    switch (state) {
      case 'distribution_in_progress':
        title = 'Referral reward detected';
        subtitle = 'Distribution in progress';
        if (newReferrals > 0) {
          subtitle += ' ($newReferrals new referral${newReferrals > 1 ? 's' : ''}, ${bonusMult}x)';
        }
        icon = Icons.pending_outlined;
        color = Colors.amberAccent;
        break;
      case 'tier_active':
        title = '$tierName tier achieved';
        subtitle = 'active till $activeUntil';
        icon = tierName == 'Pulsar' ? Icons.bolt : Icons.local_fire_department;
        color = tierName == 'Pulsar' ? Colors.cyanAccent : Colors.orangeAccent;
        break;
      default:
        return SizedBox.shrink();
    }

    return Container(
      padding: EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.08),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: color.withValues(alpha: 0.25)),
      ),
      child: Row(
        children: [
          Icon(icon, color: color, size: 20),
          SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TextStyle(color: color, fontWeight: FontWeight.bold, fontSize: 13)),
                SizedBox(height: 2),
                Text(subtitle, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 11)),
              ],
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
