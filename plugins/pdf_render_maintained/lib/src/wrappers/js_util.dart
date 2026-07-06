export './impls/js_util_stub.dart'
    if (dart.library.html) './impls/js_util_web.dart'
    if (dart.library.js) 'dart:js_util'
    if (dart.library.js_interop) './impls/js_interop.dart';
