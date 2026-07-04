export './impls/html.dart'
    if (dart.library.html) 'dart:html'
    if (dart.library.js_interop) './impls/html_wasm.dart';
