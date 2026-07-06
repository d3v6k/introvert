import 'package:flutter/material.dart';


class NodePerformanceHub extends StatelessWidget {
  const NodePerformanceHub({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text('Node Performance')),
      body: Center(
        child: Text('Telemetry Diagnostics\n(Re-implementation in progress)'),
      ),
    );
  }
}
