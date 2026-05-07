import 'package:flutter/material.dart';
import 'dart:ui';

/// Draws a high-contrast engineering grid background.
class BlueprintGridPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final gridPaint = Paint()
      ..color = Colors.white.withValues(alpha: 0.05)
      ..strokeWidth = 1;

    final subGridPaint = Paint()
      ..color = Colors.white.withValues(alpha: 0.02)
      ..strokeWidth = 0.5;

    // Main grid every 40 pixels
    for (double i = 0; i < size.width; i += 40) {
      canvas.drawLine(Offset(i, 0), Offset(i, size.height), gridPaint);
    }
    for (double i = 0; i < size.height; i += 40) {
      canvas.drawLine(Offset(0, i), Offset(size.width, i), gridPaint);
    }

    // Sub-grid every 10 pixels
    for (double i = 0; i < size.width; i += 10) {
      if (i % 40 != 0) {
        canvas.drawLine(Offset(i, 0), Offset(i, size.height), subGridPaint);
      }
    }
    for (double i = 0; i < size.height; i += 10) {
      if (i % 40 != 0) {
        canvas.drawLine(Offset(0, i), Offset(size.width, i), subGridPaint);
      }
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

/// Draws curved connecting lines between related chat bubbles based on causal links.
class BlueprintCausalPainter extends CustomPainter {
  final List<CausalLink> links;

  BlueprintCausalPainter({required this.links});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = Colors.cyanAccent.withValues(alpha: 0.2)
      ..strokeWidth = 1.5
      ..style = PaintingStyle.stroke;

    final dotPaint = Paint()
      ..color = Colors.cyanAccent.withValues(alpha: 0.4)
      ..style = PaintingStyle.fill;

    for (var link in links) {
      final path = Path();
      path.moveTo(link.start.dx, link.start.dy);
      
      // Control points for the curve
      final controlX = link.start.dx - 40;
      final controlY = (link.start.dy + link.end.dy) / 2;
      
      path.quadraticBezierTo(controlX, controlY, link.end.dx, link.end.dy);
      
      canvas.drawPath(path, paint);
      canvas.drawCircle(link.start, 2.5, dotPaint);
      canvas.drawCircle(link.end, 2.5, dotPaint);
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => true;
}

class CausalLink {
  final Offset start;
  final Offset end;
  CausalLink(this.start, this.end);
}

/// A glassmorphic chat bubble for the Blueprint UI.
class GlassmorphicBubble extends StatelessWidget {
  final String content;
  final bool isMe;
  final GlobalKey? bubbleKey;

  const GlassmorphicBubble({
    required this.content,
    required this.isMe,
    this.bubbleKey,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      key: bubbleKey,
      margin: const EdgeInsets.symmetric(vertical: 8, horizontal: 24),
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(16),
        child: BackdropFilter(
          filter: ImageFilter.blur(sigmaX: 10, sigmaY: 10),
          child: Container(
            constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
            padding: const EdgeInsets.all(16),
            decoration: BoxDecoration(
              color: isMe 
                ? Colors.cyanAccent.withValues(alpha: 0.1) 
                : Colors.white.withValues(alpha: 0.05),
              borderRadius: BorderRadius.circular(16),
              border: Border.all(
                color: Colors.white.withValues(alpha: 0.1),
                width: 1,
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  content,
                  style: const TextStyle(
                    color: Colors.white,
                    fontFamily: 'monospace',
                    fontSize: 14,
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  isMe ? "SIGNED_ED25519" : "VERIFIED_E2EE",
                  style: TextStyle(
                    color: Colors.white.withValues(alpha: 0.3),
                    fontSize: 9,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
