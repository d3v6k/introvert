import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:url_launcher/url_launcher.dart';
import '../../theme/app_theme.dart';

class UpdateService {
  static const String _defaultUrl = "https://raw.githubusercontent.com/introvert-chat/introvert/main/update.json";
  static const String _prefsUrlKey = "update_server_url";
  static const String _prefsLastCheckedKey = "update_last_checked";

  /// Gets the current update checking URL from SharedPreferences or falls back to default.
  static Future<String> getUpdateUrl() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_prefsUrlKey) ?? _defaultUrl;
  }

  /// Sets the update checking URL in SharedPreferences.
  static Future<void> setUpdateUrl(String url) async {
    final prefs = await SharedPreferences.getInstance();
    if (url.trim().isEmpty) {
      await prefs.remove(_prefsUrlKey);
    } else {
      await prefs.setString(_prefsUrlKey, url.trim());
    }
  }

  /// Reset to the default hardcoded update URL.
  static Future<void> resetToDefault() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_prefsUrlKey);
  }

  /// Triggers the check and displays the update dialog if a new version exists.
  static Future<void> checkForUpdates(BuildContext context, {bool forceShowMessage = false}) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final now = DateTime.now();

      // Unless forced, only check for updates once every 6 hours to prevent spamming requests.
      if (!forceShowMessage) {
        final lastCheckedStr = prefs.getString(_prefsLastCheckedKey);
        if (lastCheckedStr != null) {
          final lastChecked = DateTime.tryParse(lastCheckedStr);
          if (lastChecked != null && now.difference(lastChecked) < const Duration(hours: 6)) {
            debugPrint("[Update] Skipping update check (recently checked).");
            return;
          }
        }
      }

      await prefs.setString(_prefsLastCheckedKey, now.toIso8601String());
      final url = await getUpdateUrl();
      debugPrint("[Update] Checking for updates at: $url");

      final client = HttpClient();
      client.connectionTimeout = const Duration(seconds: 5);
      
      final request = await client.getUrl(Uri.parse(url));
      request.headers.set(HttpHeaders.userAgentHeader, 'IntrovertApp/1.0.0 (update-checker)');
      final response = await request.close();

      if (response.statusCode != 200) {
        debugPrint("[Update] Update server returned status ${response.statusCode}");
        client.close();
        if (forceShowMessage && context.mounted) {
          _showToast(context, "No updates found or server unreachable");
        }
        return;
      }

      final responseBody = await response.transform(utf8.decoder).join();
      client.close();

      final Map<String, dynamic> data = json.decode(responseBody);
      final String latestVersion = data['latest_version']?.toString() ?? "";
      final String downloadUrl = data['download_url']?.toString() ?? "";
      final List<dynamic> changelogRaw = data['changelog'] as List<dynamic>? ?? [];
      final List<String> changelog = changelogRaw.map((e) => e.toString()).toList();
      final bool isMandatory = data['is_mandatory'] == true;
      final String? configUrlOverride = data['config_url_override']?.toString();

      // Handle self-updating configuration URL redirection
      if (configUrlOverride != null && configUrlOverride.trim().isNotEmpty && configUrlOverride != url) {
        debugPrint("[Update] Server redirected update endpoint configuration to: $configUrlOverride");
        await setUpdateUrl(configUrlOverride);
      }

      if (latestVersion.isEmpty) return;

      final packageInfo = await PackageInfo.fromPlatform();
      final currentVersion = packageInfo.version;

      debugPrint("[Update] Current version: $currentVersion, Latest: $latestVersion");

      if (_isVersionNewer(currentVersion, latestVersion)) {
        if (context.mounted) {
          _showUpdateDialog(context, latestVersion, downloadUrl, changelog, isMandatory);
        }
      } else {
        if (forceShowMessage && context.mounted) {
          _showToast(context, "You are already running the latest version ($currentVersion)");
        }
      }
    } catch (e) {
      debugPrint("[Update] Failed to run update check: $e");
      if (forceShowMessage && context.mounted) {
        _showToast(context, "Failed to connect to update server.");
      }
    }
  }

  /// Semver comparator
  static bool _isVersionNewer(String current, String latest) {
    try {
      // Split pre-release tags (e.g. 1.0.0-beta -> 1.0.0)
      final currentClean = current.split('-').first;
      final latestClean = latest.split('-').first;

      final currentParts = currentClean.split('.').map(int.parse).toList();
      final latestParts = latestClean.split('.').map(int.parse).toList();

      final length = math.max(currentParts.length, latestParts.length);
      for (int i = 0; i < length; i++) {
        final currentPart = i < currentParts.length ? currentParts[i] : 0;
        final latestPart = i < latestParts.length ? latestParts[i] : 0;

        if (latestPart > currentPart) return true;
        if (latestPart < currentPart) return false;
      }
    } catch (e) {
      debugPrint("[Update] Error parsing semver values: $e");
    }
    return false;
  }

  /// Displays the premium update alert dialog.
  static void _showUpdateDialog(
    BuildContext context,
    String latestVersion,
    String downloadUrl,
    List<String> changelog,
    bool isMandatory,
  ) {
    showDialog(
      context: context,
      barrierDismissible: !isMandatory,
      builder: (ctx) {
        return PopScope(
          canPop: !isMandatory,
          child: BackdropFilter(
            filter: ImageFilter.blur(sigmaX: 8, sigmaY: 8),
            child: Dialog(
              backgroundColor: Colors.transparent,
              elevation: 0,
              child: Container(
                constraints: const BoxConstraints(maxWidth: 400),
                padding: EdgeInsets.all(24),
                decoration: BoxDecoration(
                  color: AppTheme.current.surface.withValues(alpha: 0.95),
                  borderRadius: BorderRadius.circular(24),
                  border: Border.all(
                    color: isMandatory ? Colors.redAccent.withValues(alpha: 0.3) : AppTheme.current.accent.withValues(alpha: 0.3),
                    width: 1.5,
                  ),
                  boxShadow: [
                    BoxShadow(
                      color: isMandatory ? Colors.redAccent.withValues(alpha: 0.1) : AppTheme.current.accent.withValues(alpha: 0.1),
                      blurRadius: 24,
                      offset: const Offset(0, 8),
                    )
                  ],
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          padding: EdgeInsets.all(8),
                          decoration: BoxDecoration(
                            color: isMandatory ? Colors.redAccent.withValues(alpha: 0.1) : AppTheme.current.accent.withValues(alpha: 0.1),
                            shape: BoxShape.circle,
                          ),
                          child: Icon(
                            isMandatory ? Icons.system_update_alt_rounded : Icons.rocket_launch_rounded,
                            color: isMandatory ? Colors.redAccent : AppTheme.current.accent,
                            size: 28,
                          ),
                        ),
                        SizedBox(width: 16),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                isMandatory ? "CRITICAL UPDATE" : "NEW UPDATE AVAILABLE",
                                style: TextStyle(
                                  color: isMandatory ? Colors.redAccent : AppTheme.current.accent,
                                  fontSize: 11,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 1.5,
                                ),
                              ),
                              SizedBox(height: 2),
                              Text(
                                "Version v$latestVersion",
                                style: TextStyle(
                                  color: AppTheme.current.text,
                                  fontSize: 16,
                                  fontWeight: FontWeight.w900,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                    SizedBox(height: 20),
                    Text(
                      "What's New:",
                      style: TextStyle(
                        color: AppTheme.current.text.withValues(alpha: 0.7),
                        fontSize: 13,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    SizedBox(height: 8),
                    if (changelog.isEmpty)
                      Text(
                        "• Performance updates and bug fixes.",
                        style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 13),
                      )
                    else
                      Container(
                        constraints: const BoxConstraints(maxHeight: 180),
                        child: ListView.builder(
                          shrinkWrap: true,
                          itemCount: changelog.length,
                          itemBuilder: (lCtx, idx) {
                            return Padding(
                              padding: EdgeInsets.only(bottom: 6),
                              child: Row(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    "• ",
                                    style: TextStyle(
                                      color: isMandatory ? Colors.redAccent : AppTheme.current.accent,
                                      fontWeight: FontWeight.bold,
                                      fontSize: 14,
                                    ),
                                  ),
                                  Expanded(
                                    child: Text(
                                      changelog[idx],
                                      style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 13),
                                    ),
                                  ),
                                ],
                              ),
                            );
                          },
                        ),
                      ),
                    if (isMandatory) ...[
                      SizedBox(height: 16),
                      Container(
                        padding: EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Colors.redAccent.withValues(alpha: 0.1),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: Colors.redAccent.withValues(alpha: 0.2)),
                        ),
                        child: Row(
                          children: [
                            Icon(Icons.warning_rounded, color: Colors.redAccent, size: 18),
                            SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                "This update is required to maintain secure mesh communication.",
                                style: TextStyle(color: Colors.redAccent, fontSize: 11, fontWeight: FontWeight.bold),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                    SizedBox(height: 24),
                    Row(
                      children: [
                        if (!isMandatory) ...[
                          Expanded(
                            child: OutlinedButton(
                              style: OutlinedButton.styleFrom(
                                side: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                                padding: EdgeInsets.symmetric(vertical: 14),
                              ),
                              onPressed: () => Navigator.pop(ctx),
                              child: Text("LATER", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontWeight: FontWeight.bold, fontSize: 13)),
                            ),
                          ),
                          SizedBox(width: 12),
                        ],
                        Expanded(
                          child: ElevatedButton(
                            style: ElevatedButton.styleFrom(
                              backgroundColor: isMandatory ? Colors.redAccent : AppTheme.current.accent,
                              shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                              padding: EdgeInsets.symmetric(vertical: 14),
                            ),
                            onPressed: () async {
                              final uri = Uri.parse(downloadUrl);
                              if (await canLaunchUrl(uri)) {
                                await launchUrl(uri, mode: LaunchMode.externalApplication);
                              }
                            },
                            child: Text(
                              "UPDATE NOW",
                              style: TextStyle(
                                color: isMandatory ? AppTheme.current.text : Colors.black,
                                fontWeight: FontWeight.bold,
                                fontSize: 13,
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  static void _showToast(BuildContext context, String msg) {
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(msg),
        duration: const Duration(seconds: 3),
        backgroundColor: AppTheme.current.surface,
      ),
    );
  }
}
