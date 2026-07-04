// This file is used to switch dart:ffi import to dummy import for Flutter Web/WASM
export 'dart:ffi'
    if (dart.library.js_interop) 'web_pointer.dart'
    if (dart.library.html) 'web_pointer.dart'
    if (dart.library.js) 'web_pointer.dart';
