import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'introvert_client.dart';


class IdentityManager {
  static const String _seedKey = 'introvert_master_seed';
  static const String _migrationKey = 'introvert_seed_migrated_to_secure';
  final IntrovertClient _client = IntrovertClient();
  final FlutterSecureStorage _secureStorage = const FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
    iOptions: IOSOptions(accessibility: KeychainAccessibility.first_unlock_this_device),
  );

  bool _useSecureStorage = true;

  /// Checks if a master seed already exists.
  Future<bool> hasIdentity() async {
    try {
      await _migrateIfNeeded();
      final seed = await _readSeed();
      return seed != null;
    } catch (e) {
      debugPrint("hasIdentity check failed: $e");
      return false;
    }
  }

  /// Retrieves the existing master seed.
  Future<Uint8List?> getSeed() async {
    try {
      await _migrateIfNeeded();
      final base64Seed = await _readSeed();
      if (base64Seed == null) return null;
      return base64Decode(base64Seed);
    } catch (e) {
      debugPrint("getSeed failed: $e");
      return null;
    }
  }

  /// Generates a new 12-word mnemonic.
  String generateMnemonic() {
    return _client.generateMnemonic();
  }

  /// Converts mnemonic to seed and saves it securely.
  Future<Uint8List> createIdentityFromMnemonic(String mnemonic) async {
    final seed = _client.mnemonicToSeed(mnemonic);
    await _writeSeed(base64Encode(seed));
    return seed;
  }

  /// Clears the identity (danger: only for testing or device resets).
  Future<void> clearIdentity() async {
    try {
      await _secureStorage.delete(key: _seedKey);
    } catch (_) {}
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_seedKey);
    await prefs.remove(_migrationKey);
  }

  /// Read seed — try secure storage first, fall back to SharedPreferences.
  Future<String?> _readSeed() async {
    if (_useSecureStorage) {
      try {
        final value = await _secureStorage.read(key: _seedKey);
        return value;
      } catch (e) {
        debugPrint("Secure storage read failed, falling back to SharedPreferences: $e");
        _useSecureStorage = false;
      }
    }
    // Fallback: read from SharedPreferences
    try {
      final prefs = await SharedPreferences.getInstance();
      return prefs.getString(_seedKey);
    } catch (e) {
      debugPrint("SharedPreferences read failed: $e");
      return null;
    }
  }

  /// Write seed — try secure storage first, fall back to SharedPreferences.
  Future<void> _writeSeed(String value) async {
    if (_useSecureStorage) {
      try {
        await _secureStorage.write(key: _seedKey, value: value);
        return;
      } catch (e) {
        debugPrint("Secure storage write failed, falling back to SharedPreferences: $e");
        _useSecureStorage = false;
      }
    }
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_seedKey, value);
  }

  /// Migrates seed from plaintext SharedPreferences to encrypted secure storage.
  /// Runs once, then marks migration as complete. Gracefully handles secure storage failures.
  Future<void> _migrateIfNeeded() async {
    final prefs = await SharedPreferences.getInstance();
    final migrated = prefs.getBool(_migrationKey) ?? false;
    if (migrated) return;

    final legacySeed = prefs.getString(_seedKey);
    if (legacySeed != null && _useSecureStorage) {
      try {
        await _secureStorage.write(key: _seedKey, value: legacySeed);
        await prefs.remove(_seedKey);
      } catch (e) {
        debugPrint("Secure storage migration failed, keeping in SharedPreferences: $e");
        _useSecureStorage = false;
      }
    }
    await prefs.setBool(_migrationKey, true);
  }
}
