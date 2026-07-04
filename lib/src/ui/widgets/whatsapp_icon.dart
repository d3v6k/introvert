import 'package:flutter/material.dart';

class WhatsAppIcon extends StatelessWidget {
  final double size;
  final Color color;

  const WhatsAppIcon({super.key, this.size = 24, this.color = Colors.white});

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: Size(size, size),
      painter: _WhatsAppPainter(color),
    );
  }
}

class _WhatsAppPainter extends CustomPainter {
  final Color color;

  _WhatsAppPainter(this.color);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.fill;

    final w = size.width;
    final h = size.height;
    final s = w / 24; // scale factor (design based on 24x24 viewBox)

    // WhatsApp-style rounded speech bubble with tail
    final path = Path()
      ..moveTo(12 * s, 2 * s)
      // Top-left curve
      ..cubicTo(6.5 * s, 2 * s, 2 * s, 6.5 * s, 2 * s, 12 * s)
      // Bottom-left curve
      ..cubicTo(2 * s, 14 * s, 2.5 * s, 15.9 * s, 3.5 * s, 17.5 * s)
      // Tail
      ..lineTo(2 * s, 22 * s)
      ..lineTo(6.5 * s, 19.5 * s)
      // Bottom-right curve
      ..cubicTo(8 * s, 20 * s, 10 * s, 20.5 * s, 12 * s, 20.5 * s)
      // Right side
      ..cubicTo(17.5 * s, 20.5 * s, 22 * s, 16 * s, 22 * s, 12 * s)
      // Top-right curve back to start
      ..cubicTo(22 * s, 6.5 * s, 17.5 * s, 2 * s, 12 * s, 2 * s)
      ..close();

    canvas.drawPath(path, paint);

    // Phone handset inside the bubble (white cutout effect)
    final handsetPaint = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8 * s
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;

    // Draw phone handset shape
    final handset = Path()
      ..moveTo(8.5 * s, 10 * s)
      ..cubicTo(8 * s, 9 * s, 8.5 * s, 8 * s, 9 * s, 7.5 * s)
      ..cubicTo(9.5 * s, 7 * s, 10 * s, 7 * s, 10.5 * s, 7.5 * s)
      ..lineTo(11 * s, 8 * s)
      ..cubicTo(11.2 * s, 8.2 * s, 11.2 * s, 8.5 * s, 11 * s, 8.7 * s)
      ..lineTo(10 * s, 10 * s)
      ..cubicTo(10 * s, 10 * s, 10.5 * s, 11.5 * s, 12 * s, 13 * s)
      ..cubicTo(13 * s, 14 * s, 14 * s, 14.5 * s, 14 * s, 14.5 * s)
      ..lineTo(15.3 * s, 13.5 * s)
      ..cubicTo(15.5 * s, 13.3 * s, 15.8 * s, 13.3 * s, 16 * s, 13.5 * s)
      ..lineTo(16.5 * s, 14 * s)
      ..cubicTo(17 * s, 14.5 * s, 17 * s, 15 * s, 16.5 * s, 15.5 * s)
      ..cubicTo(16 * s, 16.5 * s, 15 * s, 17 * s, 13.5 * s, 16.5 * s)
      ..cubicTo(11 * s, 15.5 * s, 9 * s, 13 * s, 8.5 * s, 10 * s)
      ..close();

    // Fill the handset with white
    canvas.drawPath(handset, Paint()..color = Colors.white..style = PaintingStyle.fill);
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
