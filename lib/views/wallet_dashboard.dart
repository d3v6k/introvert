import 'package:flutter/material.dart';

class WalletDashboard extends StatelessWidget {
  const WalletDashboard({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Solana Wallet')),
      body: const Center(
        child: Text('Wallet Features\n(Re-implementation in progress)'),
      ),
    );
  }
}
