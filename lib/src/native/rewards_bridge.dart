import 'dart:ffi';

/// C-compatible packed struct matching the Rust `FFIDailyState` layout.
/// Field order and types mirror #[repr(C)] exactly — no heap allocations
/// cross this boundary.
///
/// Rust layout (verified at compile time):
///   offset  0: f64  total_social_points   (8 bytes)
///   offset  8: f64  total_infra_points    (8 bytes)
///   offset 16: u32  active_web_containers (4 bytes)
///   offset 20: [pad 4 bytes for u64 alignment]
///   offset 24: u64  current_cycle_uptime  (8 bytes)
///   offset 32: u8   is_edge_node          (1 byte)
///   offset 33: u8   is_rbn                (1 byte)
///   offset 34: [pad 6 bytes to struct alignment]
///   total aligned size: 40 bytes
final class FFIDailyState extends Struct {
  @Double()
  external double totalSocialPoints;

  @Double()
  external double totalInfraPoints;

  @Uint32()
  external int activeWebContainers;

  @Uint64()
  external int currentCycleUptime;

  @Uint8()
  external int isEdgeNode;

  @Uint8()
  external int isRbn;
}

/// Native (C-side) function signature: `FFIDailyState get_current_rewards_state()`
typedef GetRewardsStateNative = FFIDailyState Function();

/// Dart-side function signature matching the native symbol.
typedef GetRewardsStateDart = FFIDailyState Function();

/// Resolves the `get_current_rewards_state` symbol from the given
/// [DynamicLibrary] and returns a typed callable.
///
/// Usage:
/// ```dart
/// final dylib = DynamicLibrary.open('libintrovert.so');
/// final getRewardsState = loadGetRewardsState(dylib);
/// final state = getRewardsState();
/// ```
GetRewardsStateDart loadGetRewardsState(DynamicLibrary dylib) {
  return dylib.lookupFunction<GetRewardsStateNative, GetRewardsStateDart>(
    'get_current_rewards_state',
  );
}

/// Convenience extension to convert raw FFI flags into readable booleans.
extension FFIDailyStateExt on FFIDailyState {
  bool get isEdge => isEdgeNode != 0;
  bool get isRbnNode => isRbn != 0;
}
