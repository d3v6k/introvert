import 'dart:convert';
import 'dart:typed_data';
import 'package:shared_preferences/shared_preferences.dart';
import 'introvert_client.dart';

class IdentityManager {
  static const String _seedKey = 'introvert_master_seed';
  final IntrovertClient _client = IntrovertClient();

  /// Checks if a master seed already exists in local storage.
  Future<bool> hasIdentity() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.containsKey(_seedKey);
  }

  /// Retrieves the existing master seed.
  Future<Uint8List?> getSeed() async {
    final prefs = await SharedPreferences.getInstance();
    final base64Seed = prefs.getString(_seedKey);
    if (base64Seed == null) return null;
    return base64Decode(base64Seed);
  }

  /// Generates a new 12-word mnemonic.
  String generateMnemonic() {
    return _client.generateMnemonic();
  }

  /// Converts mnemonic to seed and saves it securely.
  Future<Uint8List> createIdentityFromMnemonic(String mnemonic) async {
    final seed = _client.mnemonicToSeed(mnemonic);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_seedKey, base64Encode(seed));
    return seed;
  }

  /// Clears the identity (danger: only for testing or device resets).
  Future<void> clearIdentity() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_seedKey);
  }
}
