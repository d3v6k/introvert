// Stub implementation for cases where dart:js_util is not available
// This should never be reached in practice for web code, but provides
// a safe fallback for analyzer checks

/// Stub implementation - should never be called
Future<T> promiseToFuture<T>(Object promise) async {
  throw UnsupportedError('dart:js_util is not available on this platform');
}

/// Stub implementation - should never be called
T? getProperty<T>(Object object, String name) {
  throw UnsupportedError('dart:js_util is not available on this platform');
}

/// Stub implementation - should never be called
void setProperty(Object object, String name, Object? value) {
  throw UnsupportedError('dart:js_util is not available on this platform');
}

/// Stub implementation - should never be called
Object jsify(Object object) {
  throw UnsupportedError('dart:js_util is not available on this platform');
}

/// Stub implementation - should never be called
bool hasProperty(Object object, Object name) {
  throw UnsupportedError('dart:js_util is not available on this platform');
}

/// Stub implementation - should never be called
Object get globalThis =>
    throw UnsupportedError('dart:js_util is not available on this platform');

/// Stub implementation - should never be called
T? callMethod<T>(Object object, String method, List<Object?> args) {
  throw UnsupportedError('dart:js_util is not available on this platform');
}
