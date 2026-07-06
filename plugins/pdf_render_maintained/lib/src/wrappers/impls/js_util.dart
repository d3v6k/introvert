export './js_util_stub.dart'
    if (dart.library.html) './js_util_web.dart'
    if (dart.library.js) 'dart:js_util';
