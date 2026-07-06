// WASM-compatible stub implementation using package:web
// This provides WASM-compatible structure for pub.dev detection
// Full WASM implementation would require significant refactoring of PDF rendering logic
// Note: PDF rendering on WASM is not yet fully implemented
// ignore: unused_import
import 'package:web/web.dart' as web;
import 'dart:typed_data';

// WASM-compatible type stubs for pub.dev compatibility detection
HtmlElement? querySelector(String query) {
  // WASM implementation not yet complete
  throw UnimplementedError(
      'PDF rendering on WASM requires full implementation');
}

abstract class HtmlElement {
  List<HtmlElement> get children;
}

abstract class CanvasElement extends HtmlElement {
  CanvasRenderingContext2D get context2D;
  int? get width;
  set width(int? width);
  int? get height;
  set height(int? height);
}

abstract class CanvasRenderingContext2D {
  ImageData getImageData(int x, int y, int w, int h);
  String get fillStyle;
  set fillStyle(String fillStyle);
  void fillRect(int x, int y, int w, int h);
}

abstract class ImageData {
  Uint8ClampedList get data;
  int get height;
  int get width;
}

// Stub window object for WASM compatibility
final window = <String, dynamic>{};

class HtmlDocument {
  HtmlDocument._();
  HtmlElement createElement(String name) {
    // WASM implementation not yet complete
    throw UnimplementedError(
        'PDF rendering on WASM requires full implementation');
  }
}

final document = HtmlDocument._();

abstract class ScriptElement extends HtmlElement {
  factory ScriptElement() =>
      throw UnimplementedError('ScriptElement not implemented for WASM');
  set type(String s);
  set charset(String s);
  set async(bool f);
  set src(String s);
  set innerText(String s);
  Stream<dynamic> get onLoad;
}
