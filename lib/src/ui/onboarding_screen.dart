import 'package:flutter/material.dart';
import 'dart:typed_data';
import '../native/identity_manager.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';

class OnboardingScreen extends StatefulWidget {
  final Function(Uint8List seed, String avatarName) onComplete;
  final GlobalKey<ScaffoldMessengerState>? messengerKey;
  const OnboardingScreen({super.key, required this.onComplete, this.messengerKey});

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final IdentityManager _idManager = IdentityManager();
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _avatarNameController = TextEditingController();
  
  bool _isCreating = false;
  bool _showMnemonic = false;
  String? _mnemonic;

  void _showSnackBar(dynamic snackBarOrText) {
    final SnackBar snackBar = snackBarOrText is SnackBar 
        ? snackBarOrText 
        : SnackBar(content: Text(snackBarOrText.toString()));
        
    if (widget.messengerKey != null) {
      widget.messengerKey!.currentState?.showSnackBar(snackBar);
    } else if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(snackBar);
    }
  }

  @override
  void dispose() {
    _avatarNameController.dispose();
    super.dispose();
  }

  void _startCreate() {
    try {
      final mnemonic = _idManager.generateMnemonic();
      setState(() {
        _mnemonic = mnemonic;
        _showMnemonic = true;
        _avatarNameController.clear();
      });
    } on UnsupportedError catch (e) {
      _showNativeError('Native Library Unavailable', e.message ?? e.toString());
    } catch (e) {
      _showNativeError('Identity Generation Failed', e.toString());
    }
  }

  void _showNativeError(String title, String message) {
    if (!mounted) return;
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.bg,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
        title: Row(
          children: [
            Icon(Icons.warning_amber_rounded, color: Colors.orangeAccent, size: 22),
            SizedBox(width: 10),
            Expanded(child: Text(title, style: TextStyle(color: Colors.orangeAccent, fontSize: 16))),
          ],
        ),
        content: SingleChildScrollView(
          child: Text(
            message,
            style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13, height: 1.5, fontFamily: 'monospace'),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: Text('OK', style: TextStyle(color: AppTheme.current.accent)),
          ),
        ],
      ),
    );
  }

  void _startRecover() {
    showDialog<Map<String, String>>(
      context: context,
      builder: (dialogContext) => _RecoveryDialog(onSnackBar: _showSnackBar),
    ).then((result) {
      if (result != null) {
        _confirmAndCreate(result['mnemonic']!, result['avatar']!);
      }
    });
  }

  void _confirmAndCreate(String mnemonic, String avatarName) async {
    FocusManager.instance.primaryFocus?.unfocus();
    if (mnemonic.split(' ').length < 12) {
       _showSnackBar("Invalid seed phrase. Must be 12 words.");
       return;
    }

    setState(() => _isCreating = true);
    try {
      final seed = await _idManager.createIdentityFromMnemonic(mnemonic);
      
      // Check if this peer ID has a registered handle
      final peerId = _client.getPeerId();
      String? recoveredHandle;
      if (peerId != null) {
        try {
          final handleStatus = _client.getHandleStatus(peerId);
          if (handleStatus.isNotEmpty && handleStatus['handle'] != null) {
            recoveredHandle = handleStatus['handle'] as String;
          }
        } catch (_) {}
        
        // Trigger mesh DHT lookup for handle tied to this peer ID.
        // Result arrives asynchronously as Event 37 — handled by main_shell.dart
        // which persists across navigation and will update the profile automatically.
        try {
          _client.lookupPeerHandle(peerId);
        } catch (_) {}
      }
      
      widget.onComplete(seed, avatarName);
      
      // Show recovered identity info
      if (mounted && peerId != null) {
        final handleInfo = recoveredHandle != null ? '\nHandle: $recoveredHandle' : '';
        _showSnackBar(
          SnackBar(
            content: Text("Identity recovered!\nPeer ID: ${peerId.substring(0, 16)}...$handleInfo"),
            duration: const Duration(seconds: 4),
          ),
        );
      }
      
      // Safety: if callback doesn't navigate, reset spinner after delay
      Future.delayed(const Duration(seconds: 3), () {
        if (mounted && _isCreating) {
          setState(() => _isCreating = false);
        }
      });
    } catch (e) {
      if (!mounted) return;
      _showSnackBar('Identity process failed: $e');
      setState(() => _isCreating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      body: Center(
        child: SingleChildScrollView(
          padding: EdgeInsets.all(32.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Image.asset('assets/images/logo.png', height: 80),
              SizedBox(height: 16),
              Text(
                'SOVEREIGN P2P COMMUNICATION',
                style: TextStyle(fontSize: 10, fontWeight: FontWeight.bold, color: AppTheme.current.accent, letterSpacing: 1.5),
              ),
              SizedBox(height: 48),
              
              if (!_showMnemonic) ...[
                ElevatedButton(
                  onPressed: _startCreate,
                  style: ElevatedButton.styleFrom(
                    backgroundColor: AppTheme.current.accent,
                    foregroundColor: Colors.black,
                    minimumSize: const Size(double.infinity, 56),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                  ),
                  child: Text('CREATE NEW IDENTITY', style: TextStyle(fontWeight: FontWeight.bold)),
                ),
                SizedBox(height: 16),
                OutlinedButton(
                  onPressed: _startRecover,
                  style: OutlinedButton.styleFrom(
                    foregroundColor: AppTheme.current.accent,
                    side: BorderSide(color: AppTheme.current.accent),
                    minimumSize: const Size(double.infinity, 56),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                  ),
                  child: Text('RECOVER FROM SEED', style: TextStyle(fontWeight: FontWeight.bold)),
                ),
              ] else ...[
                Text(
                  'YOUR RECOVERY PHRASE',
                  style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 12, fontWeight: FontWeight.bold),
                ),
                SizedBox(height: 16),
                Container(
                  padding: EdgeInsets.all(20),
                  decoration: BoxDecoration(
                    color: AppTheme.current.text.withValues(alpha: 0.05),
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                  ),
                  child: SelectableText(
                    _mnemonic!,
                    textAlign: TextAlign.center,
                    style: TextStyle(fontSize: 16, fontFamily: 'monospace', color: AppTheme.current.accent, height: 1.5),
                  ),
                ),
                SizedBox(height: 24),
                TextField(
                  controller: _avatarNameController,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                  decoration: InputDecoration(
                    labelText: "AVATAR NAME",
                    labelStyle: TextStyle(color: AppTheme.current.accent),
                    hintText: "Enter your display name",
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                    enabledBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                    focusedBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
                  ),
                ),
                SizedBox(height: 24),
                // Intro-Claw status
                Container(
                  padding: EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                  decoration: BoxDecoration(
                    color: Colors.greenAccent.withValues(alpha: 0.06),
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.15)),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(Icons.shield_rounded, color: Colors.greenAccent, size: 16),
                      SizedBox(width: 8),
                      Text(
                        'Intro-Claw engine powering up',
                        style: TextStyle(color: Colors.greenAccent, fontSize: 11, fontWeight: FontWeight.w600),
                      ),
                    ],
                  ),
                ),
                SizedBox(height: 16),
                Text(
                  '⚠️ CAUTION: Write this down! This seed phrase is the key to your Solana wallet as well. If you lose it, your ID, wallet, and all files in the mesh drive are GONE forever. Keep it secret!',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: Colors.orangeAccent, fontSize: 12, fontWeight: FontWeight.bold, height: 1.4),
                ),
                SizedBox(height: 32),
                if (_isCreating)
                  CircularProgressIndicator(color: AppTheme.current.accent)
                else
                  ElevatedButton(
                    onPressed: () {
                      final avatar = _avatarNameController.text.trim();
                      if (avatar.isEmpty) {
                        _showSnackBar("Avatar Name is required");
                        return;
                      }
                      _confirmAndCreate(_mnemonic!, avatar);
                    },
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                      foregroundColor: AppTheme.current.accent,
                      minimumSize: const Size(double.infinity, 56),
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                    ),
                    child: Text('I HAVE SECURED MY SEED', style: TextStyle(fontWeight: FontWeight.bold)),
                  ),
                TextButton(
                  onPressed: () => setState(() => _showMnemonic = false),
                  child: Text("GO BACK", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _RecoveryDialog extends StatefulWidget {
  final void Function(dynamic) onSnackBar;
  const _RecoveryDialog({required this.onSnackBar});

  @override
  State<_RecoveryDialog> createState() => _RecoveryDialogState();
}

class _RecoveryDialogState extends State<_RecoveryDialog> {
  final TextEditingController _recoveryController = TextEditingController();
  final TextEditingController _avatarController = TextEditingController();

  @override
  void dispose() {
    _recoveryController.dispose();
    _avatarController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.current.bg,
      title: Text("Recover Identity", style: TextStyle(color: AppTheme.current.accent)),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text("Enter your 12-word seed phrase and an avatar name below to restore your sovereign ID.", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12)),
            SizedBox(height: 16),
            TextField(
              controller: _recoveryController,
              maxLines: 3,
              style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace', fontSize: 14),
              decoration: InputDecoration(
                hintText: "word1 word2 ... word12",
                hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                enabledBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
                focusedBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
              ),
            ),
            SizedBox(height: 16),
            TextField(
              controller: _avatarController,
              style: TextStyle(color: AppTheme.current.text, fontSize: 14),
              decoration: InputDecoration(
                labelText: "AVATAR NAME",
                labelStyle: TextStyle(color: AppTheme.current.accent),
                hintText: "Enter display name",
                hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                enabledBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
                focusedBorder: OutlineInputBorder(borderSide: BorderSide(color: AppTheme.current.accent)),
              ),
            ),
            SizedBox(height: 12),
            Text(
              "Your peer ID and i@ handle (if registered) will be restored automatically after login.",
              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 11),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: Text("CANCEL")),
        ElevatedButton(
          onPressed: () {
            final mnemonic = _recoveryController.text.trim();
            final avatar = _avatarController.text.trim();
            if (mnemonic.isEmpty) return;
            if (avatar.isEmpty) {
              widget.onSnackBar("Avatar Name is required");
              return;
            }
            Navigator.pop(context, {
              'mnemonic': mnemonic,
              'avatar': avatar,
            });
          },
          style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
          child: Text("RECOVER"),
        ),
      ],
    );
  }
}
