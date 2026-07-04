// ignore_for_file: deprecated_member_use, uri_does_not_exist

import 'dart:js_util' as js_util;

export 'dart:js_util'
    show
        promiseToFuture,
        getProperty,
        setProperty,
        jsify,
        hasProperty,
        globalThis,
        callMethod;

// Provide a namespace alias so callers can prefix-import as `js_util`. This
// avoids breaking existing code that expects the members to be on the prefix.
// ignore: camel_case_types
class js_util_ns {
  static T? getProperty<T>(Object o, String name) =>
      js_util.getProperty<T>(o, name);
  static void setProperty(Object o, String name, Object? value) {
    js_util.setProperty(o, name, value);
  }

  static Object jsify(Object o) => js_util.jsify(o);
  static Future<T> promiseToFuture<T>(Object promise) =>
      js_util.promiseToFuture<T>(promise);
  static bool hasProperty(Object o, Object name) =>
      js_util.hasProperty(o, name);
  static Object get globalThis => js_util.globalThis;
  static T? callMethod<T>(Object o, String method, List<Object?> args) =>
      js_util.callMethod<T>(o, method, args);
}
