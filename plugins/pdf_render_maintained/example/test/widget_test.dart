// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility that Flutter provides. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:flutter_test/flutter_test.dart';
import 'package:pdf_render_example/main.dart';

void main() {
  testWidgets('Verify app loads correctly', (WidgetTester tester) async {
    // Build our app and trigger a frame.
    await tester.pumpWidget(const PdfRenderExampleApp());

    // Verify that the list of demos is displayed.
    expect(find.text('pdf_render_maintained samples'), findsOneWidget);

    // Verify that at least one demo tile exists.
    expect(find.text('Asset viewer'), findsOneWidget);
  });
}
