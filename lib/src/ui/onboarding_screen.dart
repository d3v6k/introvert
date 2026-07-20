import 'dart:async';
import 'package:flutter/material.dart';
import 'dart:typed_data';
import '../native/identity_manager.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';

class OnboardingScreen extends StatefulWidget {
  final Function(Uint8List seed, String avatarName) onComplete;
  final GlobalKey<ScaffoldMessengerState>? messengerKey;
  const OnboardingScreen({super.key, required this.onComplete, this.messengerKey});

  /// Set by _confirmAndCreate when no handle was found for a recovered identity.
  /// main_shell.dart checks this after app load and shows handle creation dialog.
  static bool needsHandlePrompt = false;
  static String? promptPeerId;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final IdentityManager _idManager = IdentityManager();
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _avatarNameController = TextEditingController();
  
  bool _isCreating = false;
  bool _showMnemonic = false;
  bool _isRecovery = false;
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
        _isRecovery = false;
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
    _isRecovery = true;
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
      final peerId = _client.getPeerId();

      // For recovery: check if handle exists before prompting
      String? recoveredHandle;
      if (_isRecovery && peerId != null) {
        // Check local DB first
        try {
          final handleStatus = _client.getHandleStatus(peerId);
          if (handleStatus.isNotEmpty && handleStatus['handle'] != null) {
            recoveredHandle = handleStatus['handle'] as String;
          }
        } catch (_) {}

        // Trigger DHT lookup and wait up to 8 seconds
        if (recoveredHandle == null) {
          try {
            _client.lookupPeerHandle(peerId);
          } catch (_) {}
          recoveredHandle = await _waitForDhtHandle(peerId, timeout: const Duration(seconds: 8));
        }
      }

      // Start the engine and navigate to main app
      widget.onComplete(seed, avatarName);

      // Set static flag for main_shell to pick up after navigation
      if (recoveredHandle != null) {
        // Handle found — no prompt needed
      } else {
        // No handle found (new identity or recovery without handle) — schedule prompt
        OnboardingScreen.needsHandlePrompt = true;
        OnboardingScreen.promptPeerId = peerId;
      }

      // Safety: reset spinner after delay
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

  Future<String?> _waitForDhtHandle(String peerId, {Duration timeout = const Duration(seconds: 8)}) async {
    final completer = Completer<String?>();
    late StreamSubscription sub;
    sub = _client.networkStream.listen((event) {
      if (event.type == 37) {
        try {
          final data = String.fromCharCodes(event.data);
          final parts = data.split('\x00');
          if (parts.length >= 2 && parts[0] == peerId) {
            final handle = parts[1];
            if (handle.isNotEmpty && !completer.isCompleted) {
              completer.complete(handle);
            }
          }
        } catch (_) {}
      }
    });

    // Also check if handle was already set by main_shell.dart listener
    Future.delayed(const Duration(seconds: 1), () {
      if (!completer.isCompleted) {
        try {
          final handleStatus = _client.getHandleStatus(peerId);
          if (handleStatus.isNotEmpty && handleStatus['handle'] != null) {
            completer.complete(handleStatus['handle'] as String);
          }
        } catch (_) {}
      }
    });

    Future.delayed(timeout, () {
      if (!completer.isCompleted) completer.complete(null);
    });

    final result = await completer.future;
    sub.cancel();
    return result;
  }




  /// Show a dialog prompting the user to create an introvert handle.
  /// Can be called from main_shell after navigation completes.
  static void showHandleCreationPrompt(BuildContext context, String? peerId) {
    final handleController = TextEditingController();
    bool isClaiming = false;
    String? claimStatus;
    final client = IntrovertClient();

    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (ctx) {
        return StatefulBuilder(
          builder: (ctx, setDialogState) {
            return AlertDialog(
              backgroundColor: AppTheme.current.surface,
              title: Row(
                children: [
                  Icon(Icons.alternate_email_rounded, color: AppTheme.current.accent, size: 20),
                  SizedBox(width: 8),
                  Text(
                    "CHOOSE YOUR HANDLE",
                    style: TextStyle(
                      color: AppTheme.current.text,
                      fontFamily: 'monospace',
                      fontSize: 14,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ],
              ),
              content: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      "Choose a unique handle for your identity. This cannot be changed later.",
                      style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12, height: 1.4),
                    ),
                    SizedBox(height: 16),
                    TextField(
                      controller: handleController,
                      style: TextStyle(color: AppTheme.current.text, fontFamily: 'monospace', fontSize: 14),
                      decoration: InputDecoration(
                        prefixText: 'i@',
                        prefixStyle: TextStyle(color: AppTheme.current.accent, fontFamily: 'monospace', fontSize: 14),
                        hintText: "yourname",
                        hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3)),
                        enabledBorder: OutlineInputBorder(
                          borderSide: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.3)),
                        ),
                        focusedBorder: OutlineInputBorder(
                          borderSide: BorderSide(color: AppTheme.current.accent),
                        ),
                      ),
                      enabled: !isClaiming,
                    ),
                    if (claimStatus != null) ...[
                      SizedBox(height: 12),
                      Container(
                        padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                        decoration: BoxDecoration(
                          color: claimStatus!.startsWith('verified')
                              ? Colors.greenAccent.withValues(alpha: 0.08)
                              : claimStatus!.startsWith('error')
                                  ? Colors.redAccent.withValues(alpha: 0.08)
                                  : Colors.amberAccent.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(8),
                          border: Border.all(
                            color: claimStatus!.startsWith('verified')
                                ? Colors.greenAccent.withValues(alpha: 0.3)
                                : claimStatus!.startsWith('error')
                                    ? Colors.redAccent.withValues(alpha: 0.3)
                                    : Colors.amberAccent.withValues(alpha: 0.3),
                          ),
                        ),
                        child: Row(
                          children: [
                            if (isClaiming)
                              SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent))
                            else
                              Icon(
                                claimStatus!.startsWith('verified')
                                    ? Icons.check_circle_rounded
                                    : Icons.info_outline_rounded,
                                size: 16,
                                color: claimStatus!.startsWith('verified')
                                    ? Colors.greenAccent
                                    : claimStatus!.startsWith('error')
                                        ? Colors.redAccent
                                        : Colors.amberAccent,
                              ),
                            SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                claimStatus!.startsWith('verified')
                                    ? 'Handle verified!'
                                    : claimStatus!,
                                style: TextStyle(fontSize: 11, color: AppTheme.current.text.withValues(alpha: 0.7)),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                    SizedBox(height: 12),
                    Text(
                      '\u26a0\ufe0f Handles are permanent and cannot be changed once claimed.',
                      style: TextStyle(color: Colors.orangeAccent.withValues(alpha: 0.7), fontSize: 10, height: 1.3),
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: isClaiming ? null : () {
                    Navigator.pop(ctx);
                  },
                  child: Text("SKIP", style: TextStyle(color: AppTheme.current.mutedText)),
                ),
                ElevatedButton(
                  onPressed: isClaiming ? null : () async {
                    final handle = handleController.text.trim();
                    if (handle.isEmpty) {
                      setDialogState(() => claimStatus = 'Please enter a handle');
                      return;
                    }
                    if (!RegExp(r'^[a-zA-Z0-9_]{3,64}$').hasMatch(handle)) {
                      setDialogState(() => claimStatus = 'error: 3-64 chars, letters/numbers/underscores only');
                      return;
                    }

                    setDialogState(() {
                      isClaiming = true;
                      claimStatus = 'Generating proof of work...';
                    });

                    try {
                      client.claimHandle('i@$handle');
                      setDialogState(() => claimStatus = 'Claiming handle — waiting for RBN verification...');

                      // Poll for verification
                      bool verified = false;
                      for (int i = 0; i < 10; i++) {
                        await Future.delayed(const Duration(seconds: 3));
                        if (!ctx.mounted) break;
                        try {
                          final status = client.getHandleStatus(client.getPeerId() ?? '');
                          if (status.isNotEmpty && status['verified'] == true) {
                            verified = true;
                            break;
                          }
                        } catch (_) {}
                      }

                      if (verified) {
                        setDialogState(() => claimStatus = 'verified: i@$handle');
                        client.setProfile(null, 'i@$handle', null, 1);
                        Future.delayed(const Duration(seconds: 2), () {
                          if (ctx.mounted) Navigator.pop(ctx);
                        });
                      } else {
                        setDialogState(() {
                          isClaiming = false;
                          claimStatus = 'error: Verification timed out. Try again from Profile tab.';
                        });
                      }
                    } catch (e) {
                      setDialogState(() {
                        isClaiming = false;
                        claimStatus = 'error: $e';
                      });
                    }
                  },
                  style: ElevatedButton.styleFrom(
                    backgroundColor: AppTheme.current.accent,
                    foregroundColor: Colors.black,
                  ),
                  child: Text(isClaiming ? 'CLAIMING...' : 'CLAIM HANDLE'),
                ),
              ],
            );
          },
        );
      },
    );
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
