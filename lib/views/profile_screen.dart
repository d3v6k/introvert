import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';

import '../src/native/introvert_client.dart';
import '../src/ui/widgets/network_optimization_button.dart';
import '../src/ui/widgets/rewards_hud.dart';
import '../blueprint_ui.dart';
import 'tier_preview_screen.dart';
import 'theme_mockup_grid.dart';
import '../theme/app_theme.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key});

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _handleController = TextEditingController();
  String? _base64Avatar;
  int _privacyMode = 1; // Default: 1 (Extroverted / Allowed)
  bool _isSaving = false;
  bool _isClaimed = false;
  bool _isClaiming = false;
  bool _hasExistingHandle = false;
  bool _isDisposing = false;
  StreamSubscription? _networkSubscription;
  StreamSubscription? _economySubscription;

  Map<String, dynamic> _economyStats = {
    'intr_balance': 0,
    'pending_rewards': 0,
    'total_relayed': 0,
    'sol_address': '...',
  };

  @override
  void initState() {
    super.initState();
    _handleController.addListener(_onHandleChanged);
    _loadProfile();
    _startEconomyMonitor();
    _startNetworkListener();
  }

  @override
  void dispose() {
    _isDisposing = true;
    _handleController.removeListener(_onHandleChanged);
    _economySubscription?.cancel();
    _networkSubscription?.cancel();
    super.dispose();
  }

  void _onHandleChanged() {
    final h = _handleController.text.trim();
    if (h.isEmpty) {
      if (mounted) {
        setState(() {
          _isClaimed = false;
          _isClaiming = false;
        });
      }
      return;
    }
    final fullHandle = "i@$h";
    final status = IntrovertClient().getHandleStatus(fullHandle);
    if (mounted) {
      setState(() {
        _isClaimed = status['verified'] == true;
      });
    }
  }

  void _startNetworkListener() {
    _networkSubscription = IntrovertClient().networkStream.listen((event) {
      if (_isDisposing) return;
      if (event.type == 34) {
        // Event 34: Handle Verified [Handle\0PeerID]
        try {
          final parts = utf8.decode(event.data).split('\x00');
          var receivedHandle = parts[0];
          if (receivedHandle.startsWith("i@")) {
            receivedHandle = receivedHandle.substring(2);
          }
          if (receivedHandle == _handleController.text.trim()) {
            if (mounted) {
              setState(() {
                _isClaimed = true;
                _isClaiming = false;
              });
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text("Handle officially verified by the mesh!")),
              );
            }
          }
        } catch (_) {}
      } else if (event.type == 37) {
        // Event 37: Peer Handle Restored/Resolved [PID\0Handle]
        try {
          final parts = utf8.decode(event.data).split('\x00');
          if (parts.length >= 2) {
            _loadProfile();
          }
        } catch (_) {}
      }
    });
  }

  void _startEconomyMonitor() {
    final client = IntrovertClient();
    _economySubscription = client.economyStream.listen((stats) {
      if (mounted) {
        setState(() => _economyStats = stats);
      }
    });
  }

  void _loadProfile() {
    final client = IntrovertClient();
    final profile = client.getProfile();
    setState(() {
      _nameController.text = profile['name'] ?? '';
      String h = profile['handle'] ?? '';
      if (h.startsWith("i@")) h = h.substring(2);
      _handleController.text = h;
      _hasExistingHandle = h.isNotEmpty;
      _base64Avatar = profile['avatar'];
      _privacyMode = profile['privacy_mode'] ?? 1; // Default to allowed
      
      final fullHandle = h.isNotEmpty ? "i@$h" : "";
      if (fullHandle.isNotEmpty) {
        final status = client.getHandleStatus(fullHandle);
        _isClaimed = status['verified'] == true;
      }
    });
  }

  Future<void> _pickImage() async {
    final picker = ImagePicker();
    final image = await picker.pickImage(source: ImageSource.gallery, maxWidth: 300, maxHeight: 300);
    
    if (image != null) {
      final bytes = await image.readAsBytes();
      setState(() {
        _base64Avatar = base64Encode(bytes);
      });
    }
  }

  void _claimHandle() {
    final h = _handleController.text.trim();
    if (h.isEmpty) return;
    setState(() => _isClaiming = true);
    IntrovertClient().claimHandle(h);
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text("Claim initiated. Generating Proof-of-Work and gossiping to RBNs...")),
    );

    // Poll for verification: check handle status every 5 seconds for up to 60 seconds
    int attempts = 0;
    Timer.periodic(const Duration(seconds: 5), (timer) {
      attempts++;
      if (!mounted || !_isClaiming) {
        timer.cancel();
        return;
      }
      try {
        final fullHandle = "i@${_handleController.text.trim()}";
        final status = IntrovertClient().getHandleStatus(fullHandle);
        if (status.isNotEmpty && status['verified'] == true) {
          timer.cancel();
          if (mounted) {
            setState(() {
              _isClaimed = true;
              _isClaiming = false;
            });
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text("Handle officially verified by the mesh!")),
            );
          }
        }
      } catch (_) {}

      if (attempts >= 12) { // 60 seconds total
        timer.cancel();
        if (mounted && _isClaiming) {
          setState(() => _isClaiming = false);
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text("Handle claim timed out. Please check network and try again.")),
          );
        }
      }
    });
  }

  void _saveProfile() async {
    setState(() => _isSaving = true);
    final client = IntrovertClient();
    
    String h = _handleController.text.trim();
    String handle = h.isNotEmpty ? "i@$h" : "";

    try {
      client.setProfile(_nameController.text.trim(), handle, _base64Avatar, _privacyMode);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Profile updated successfully.')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to update profile: $e')),
        );
      }
    } finally {
      if (mounted) setState(() => _isSaving = false);
    }
  }

  String _tierName(double balance) {
    if (balance >= 1000000) return 'PLATINUM';
    if (balance >= 500000) return 'GOLD';
    if (balance >= 250000) return 'SILVER';
    if (balance >= 100000) return 'SENTINEL';
    return 'CITIZEN';
  }

  Future<void> _claimRewards() async {
    final client = IntrovertClient();
    try {
      final sig = await client.claimRewards();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text("Claim Successful! Sig: ${sig.substring(0, 8)}..."),
            backgroundColor: AppTheme.current.accent,
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Claim failed: $e"), backgroundColor: Colors.redAccent),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final balance = (_economyStats['intr_balance'] ?? 0) / 1000000000.0;
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        title: Text('PROFILE', style: TextStyle(fontFamily: 'monospace', letterSpacing: 2)),
        backgroundColor: Colors.transparent,
        elevation: 0,
        actions: [
          NetworkOptimizationButton(),
        ],
      ),
      body: SingleChildScrollView(
        padding: EdgeInsets.all(24),
        child: Column(
          children: [
            GestureDetector(
              onTap: _pickImage,
              child: SovereignAvatar(
                radius: 90,
                balance: balance,
                avatar: _base64Avatar != null && _base64Avatar!.isNotEmpty
                    ? MemoryImage(base64Decode(_base64Avatar!)) 
                    : null,
                isSuperActive: (_economyStats['total_relayed'] ?? 0) > 100 * 1024 * 1024,
              ),
            ),
            SizedBox(height: 12),
            Text(
              'Current Level: ${_tierName(balance)}',
              style: TextStyle(color: AppTheme.current.accent, fontSize: 14, fontWeight: FontWeight.bold, fontFamily: 'monospace', letterSpacing: 1),
            ),
            SizedBox(height: 4),
            Text(
              'INTR Balance: ${balance.toStringAsFixed(2)}',
              style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12, fontFamily: 'monospace'),
            ),
            SizedBox(height: 8),
            TextButton(
              onPressed: () => Navigator.push(context, MaterialPageRoute(builder: (context) => const TierPreviewScreen())),
              child: Text('VIEW PRESTIGE TIERS', style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontFamily: 'monospace', letterSpacing: 1)),
            ),
            SizedBox(height: 16),
            TextField(
              controller: _nameController,
              style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace'),
              decoration: InputDecoration(
                labelText: 'GLOBAL ALIAS',
                labelStyle: TextStyle(color: AppTheme.current.accent, fontSize: 12),
                enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
              ),
            ),
            SizedBox(height: 24),
            TextField(
              controller: _handleController,
              readOnly: _isClaimed || _hasExistingHandle,
              style: TextStyle(
                color: (_isClaimed || _hasExistingHandle) ? AppTheme.current.accent : AppTheme.current.text,
                fontFamily: 'monospace',
              ),
              decoration: InputDecoration(
                labelText: 'INTROVERT HANDLE',
                labelStyle: TextStyle(color: AppTheme.current.accent, fontSize: 12),
                hintText: (_isClaimed || _hasExistingHandle) ? null : 'username',
                hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                prefixText: 'i@',
                prefixStyle: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold),
                enabledBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                focusedBorder: UnderlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
                suffixIcon: (_isClaimed || _hasExistingHandle)
                  ? Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(Icons.lock_outline, color: AppTheme.current.accent, size: 16),
                        SizedBox(width: 4),
                        Icon(Icons.verified, color: AppTheme.current.accent, size: 20),
                      ],
                    )
                  : (_handleController.text.isNotEmpty ? Icon(Icons.new_releases_outlined, color: Colors.orangeAccent, size: 20) : null),
              ),
            ),
            if (_isClaimed || _hasExistingHandle) ...[
              SizedBox(height: 6),
              Text(
                'Handle is permanently locked to your identity. Immutable on-chain.',
                style: TextStyle(color: AppTheme.current.accent.withValues(alpha: 0.6), fontSize: 10, fontStyle: FontStyle.italic),
              ),
            ],
            if (!_isClaimed && !_hasExistingHandle && _handleController.text.isNotEmpty) ...[
              SizedBox(height: 12),
              Align(
                alignment: Alignment.centerRight,
                child: _isClaiming 
                  ? SizedBox(width: 20, height: 20, child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent))
                  : TextButton.icon(
                      onPressed: _claimHandle,
                      icon: Icon(Icons.how_to_reg, size: 16, color: AppTheme.current.accent),
                      label: Text("CLAIM & VERIFY", style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold)),
                      style: TextButton.styleFrom(backgroundColor: AppTheme.current.text.withValues(alpha: 0.05)),
                    ),
              ),
            ],
            SizedBox(height: 32),
            Container(
              decoration: BoxDecoration(
                color: AppTheme.current.text.withValues(alpha: 0.05),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
              ),
              child: Material(
                color: Colors.transparent,
                child: SwitchListTile(
                  title: Text(
                    "Allow unknown users to connect",
                    style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: Text(
                    "If disabled, you can only be reached via Magic Links. Highly private.",
                    style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  value: _privacyMode == 1,
                  activeThumbColor: AppTheme.current.accent,
                  onChanged: (val) {
                    setState(() => _privacyMode = val ? 1 : 0);
                  },
                ),
              ),
            ),
            SizedBox(height: 32),
            if (_isSaving)
              CircularProgressIndicator(color: AppTheme.current.accent)
            else
              ElevatedButton(
                onPressed: _saveProfile,
                style: ElevatedButton.styleFrom(
                  backgroundColor: AppTheme.current.accent,
                  foregroundColor: Colors.black,
                  minimumSize: const Size(double.infinity, 50),
                  shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                ),
                child: Text('SAVE IDENTITY', style: TextStyle(fontWeight: FontWeight.bold)),
              ),
            SizedBox(height: 40),
            SovereignEarnings(
              economyStats: _economyStats,
              onClaim: _claimRewards,
            ),
            SizedBox(height: 40),
            ElevatedButton(
              onPressed: () {
                Navigator.push(
                  context,
                  MaterialPageRoute(builder: (context) => const ThemeMockupGridScreen()),
                );
              },
              style: ElevatedButton.styleFrom(
                backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                foregroundColor: AppTheme.current.text,
                minimumSize: const Size(double.infinity, 50),
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
              ),
              child: Text('VIEW THEME MOCKUPS', style: TextStyle(fontWeight: FontWeight.bold, letterSpacing: 1.2)),
            ),
            SizedBox(height: 40),
          ],
        ),
      ),
    );
  }
}
