import 'package:flutter/material.dart';


class WalletDashboard extends StatelessWidget {
  const WalletDashboard({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text('Solana Wallet')),
      body: Center(
        child: Text('Wallet Features\n(Re-implementation in progress)'),
      ),
    );
  }
}
