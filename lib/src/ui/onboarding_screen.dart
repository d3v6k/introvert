import 'package:flutter/material.dart';
import 'dart:typed_data';
import '../native/identity_manager.dart';

class OnboardingScreen extends StatefulWidget {
  final Function(Uint8List seed) onComplete;
  final GlobalKey<ScaffoldMessengerState>? messengerKey;
  const OnboardingScreen({super.key, required this.onComplete, this.messengerKey});

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final IdentityManager _idManager = IdentityManager();
  String? _mnemonic;
  bool _isCreating = false;

  @override
  void initState() {
    super.initState();
    _mnemonic = _idManager.generateMnemonic();
  }

  void _confirmAndCreate() async {
    setState(() => _isCreating = true);
    try {
      final seed = await _idManager.createIdentityFromMnemonic(_mnemonic!);
      widget.onComplete(seed);
    } catch (e) {
      if (!mounted) return;
      
      final snackBar = SnackBar(content: Text('Identity creation failed: $e'));
      if (widget.messengerKey != null) {
        widget.messengerKey!.currentState?.showSnackBar(snackBar);
      } else {
        ScaffoldMessenger.of(context).showSnackBar(snackBar);
      }
      
      setState(() => _isCreating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(32.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.security, size: 64, color: Color(0xFF00FFFF)),
              const SizedBox(height: 24),
              const Text(
                'Welcome to Introvert',
                style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold, color: Colors.white),
              ),
              const SizedBox(height: 16),
              const Text(
                'This is your unique recovery phrase. Write it down and keep it in a safe place. It is the only way to recover your account.',
                textAlign: TextAlign.center,
                style: TextStyle(color: Colors.grey),
              ),
              const SizedBox(height: 32),
              Container(
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: Colors.white.withValues(alpha: 0.05),
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: const Color(0xFF00FFFF).withValues(alpha: 0.3)),
                ),
                child: SelectableText(
                  _mnemonic ?? 'Generating...',
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    fontSize: 18,
                    fontFamily: 'monospace',
                    color: Color(0xFF00FFFF),
                  ),
                ),
              ),
              const SizedBox(height: 48),
              if (_isCreating)
                const CircularProgressIndicator()
              else
                ElevatedButton(
                  onPressed: _confirmAndCreate,
                  style: ElevatedButton.styleFrom(
                    backgroundColor: const Color(0xFF00FFFF),
                    foregroundColor: Colors.black,
                    padding: const EdgeInsets.symmetric(horizontal: 48, vertical: 16),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(30)),
                  ),
                  child: const Text('I have written it down', style: TextStyle(fontWeight: FontWeight.bold)),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
