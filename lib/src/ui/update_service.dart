import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:open_file/open_file.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:url_launcher/url_launcher.dart';
import '../../theme/app_theme.dart';

class UpdateService {
  static const String _githubApiUrl = "https://api.github.com/repos/d3v6k/introvert/releases/latest";
  static const String _prefsLastCheckedKey = "update_last_checked";

  /// Gets the latest release info from GitHub.
  static Future<Map<String, dynamic>?> _fetchLatestRelease() async {
    final client = HttpClient();
    client.connectionTimeout = const Duration(seconds: 10);
    try {
      final request = await client.getUrl(Uri.parse(_githubApiUrl));
      request.headers.set(HttpHeaders.userAgentHeader, 'IntrovertApp/1.0.0 (update-checker)');
      request.headers.set(HttpHeaders.acceptHeader, 'application/vnd.github.v3+json');
      final response = await request.close();

      if (response.statusCode != 200) {
        debugPrint("[Update] GitHub API returned status ${response.statusCode}");
        return null;
      }

      final body = await response.transform(utf8.decoder).join();
      return json.decode(body) as Map<String, dynamic>;
    } finally {
      client.close();
    }
  }

  /// Strips 'v' prefix from tag: "v0.33.0" -> "0.33.0"
  static String _normalizeVersion(String tag) {
    return tag.startsWith('v') ? tag.substring(1) : tag;
  }

  /// Returns the platform-appropriate asset name pattern.
  static String? _platformAssetPattern() {
    if (Platform.isAndroid) return 'app-release.apk';
    if (Platform.isMacOS) return '.dmg';
    if (Platform.isLinux) return '.tar.gz';
    if (Platform.isWindows) return '.exe';
    return null; // iOS cannot self-update
  }

  /// Finds the download URL for the current platform from release assets.
  static String? _findPlatformDownloadUrl(Map<String, dynamic> release) {
    final assets = release['assets'] as List<dynamic>? ?? [];
    final pattern = _platformAssetPattern();
    if (pattern == null) return null;

    for (final asset in assets) {
      final name = (asset['name'] as String? ?? '').toLowerCase();
      if (pattern == 'app-release.apk') {
        if (name == 'app-release.apk') return asset['browser_download_url'] as String?;
      } else if (name.endsWith(pattern)) {
        return asset['browser_download_url'] as String?;
      }
    }
    return null;
  }

  /// Triggers the check and displays the update dialog if a new version exists.
  static Future<void> checkForUpdates(BuildContext context, {bool forceShowMessage = false}) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final now = DateTime.now();

      // Unless forced, only check once every 6 hours
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
      debugPrint("[Update] Checking GitHub releases for updates...");

      final release = await _fetchLatestRelease();
      if (release == null) {
        if (forceShowMessage && context.mounted) {
          _showToast(context, "Could not reach update server.");
        }
        return;
      }

      final String tagName = (release['tag_name'] as String?) ?? '';
      final String latestVersion = _normalizeVersion(tagName);
      final String releaseNotes = (release['body'] as String?) ?? '';
      final List<String> changelog = releaseNotes
          .split('\n')
          .map((l) => l.trim())
          .where((l) => l.isNotEmpty && (l.startsWith('-') || l.startsWith('*')))
          .map((l) => l.replaceFirst(RegExp(r'^[-*]\s*'), ''))
          .toList();

      if (latestVersion.isEmpty) return;

      final packageInfo = await PackageInfo.fromPlatform();
      final currentVersion = packageInfo.version;

      debugPrint("[Update] Current: $currentVersion, Latest: $latestVersion");

      if (_isVersionNewer(currentVersion, latestVersion)) {
        final downloadUrl = _findPlatformDownloadUrl(release);
        if (context.mounted) {
          _showUpdateDialog(context, latestVersion, downloadUrl, changelog);
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
      debugPrint("[Update] Error parsing semver: $e");
    }
    return false;
  }

  /// Downloads a file from [url] to a temp directory, reporting progress via [onProgress].
  /// Returns the downloaded file path on success, null on failure.
  static Future<String?> _downloadFile(String url, void Function(double progress) onProgress) async {
    final client = HttpClient();
    client.connectionTimeout = const Duration(seconds: 15);
    try {
      final request = await client.getUrl(Uri.parse(url));
      final response = await request.close();

      if (response.statusCode != 200 && response.statusCode != 302) {
        debugPrint("[Update] Download failed: HTTP ${response.statusCode}");
        return null;
      }

      final contentLength = response.contentLength;
      final dir = await getTemporaryDirectory();
      final fileName = url.split('/').last.split('?').first;
      final filePath = '${dir.path}/$fileName';
      final file = File(filePath);
      final sink = file.openWrite();

      int downloaded = 0;
      await for (final chunk in response) {
        sink.add(chunk);
        downloaded += chunk.length;
        if (contentLength > 0) {
          onProgress(downloaded / contentLength);
        }
      }
      await sink.flush();
      await sink.close();

      debugPrint("[Update] Downloaded to $filePath (${downloaded ~/ 1024} KB)");
      return filePath;
    } finally {
      client.close();
    }
  }

  /// Opens a downloaded file — APK triggers install on Android, DMG opens on macOS.
  static Future<void> _openDownloadedFile(String filePath) async {
    try {
      if (Platform.isAndroid) {
        // On Android 10+, we need to open via content URI for package installer
        // open_file handles this
        await OpenFile.open(filePath);
      } else if (Platform.isMacOS) {
        await Process.run('open', [filePath]);
      } else if (Platform.isLinux) {
        await Process.run('xdg-open', [filePath]);
      } else if (Platform.isWindows) {
        await Process.run('start', [filePath], runInShell: true);
      }
    } catch (e) {
      debugPrint("[Update] Failed to open file: $e");
    }
  }

  /// Shows the update dialog with version info, changelog, and a download button.
  static void _showUpdateDialog(
    BuildContext context,
    String latestVersion,
    String? downloadUrl,
    List<String> changelog,
  ) {
    final hasDownload = downloadUrl != null && downloadUrl.isNotEmpty;
    final platformPattern = _platformAssetPattern();
    final platformLabel = platformPattern == 'app-release.apk'
        ? 'APK'
        : platformPattern == '.dmg'
            ? 'macOS DMG'
            : platformPattern == '.tar.gz'
                ? 'Linux'
                : platformPattern == '.exe'
                    ? 'Windows'
                    : 'your platform';

    showDialog(
      context: context,
      barrierDismissible: true,
      builder: (ctx) {
        return _UpdateDialogContent(
          latestVersion: latestVersion,
          downloadUrl: downloadUrl,
          changelog: changelog,
          hasDownload: hasDownload,
          platformLabel: platformLabel,
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

/// Separate StatefulWidget for the update dialog so download progress
/// state doesn't require rebuilding the entire UpdateService.
class _UpdateDialogContent extends StatefulWidget {
  final String latestVersion;
  final String? downloadUrl;
  final List<String> changelog;
  final bool hasDownload;
  final String platformLabel;

  const _UpdateDialogContent({
    required this.latestVersion,
    required this.downloadUrl,
    required this.changelog,
    required this.hasDownload,
    required this.platformLabel,
  });

  @override
  State<_UpdateDialogContent> createState() => _UpdateDialogContentState();
}

class _UpdateDialogContentState extends State<_UpdateDialogContent> {
  bool _downloading = false;
  double _progress = 0.0;
  String? _error;

  Future<void> _startDownload() async {
    if (widget.downloadUrl == null) return;

    setState(() {
      _downloading = true;
      _progress = 0.0;
      _error = null;
    });

    try {
      final filePath = await UpdateService._downloadFile(
        widget.downloadUrl!,
        (p) => setState(() => _progress = p),
      );

      if (filePath != null) {
        setState(() => _progress = 1.0);
        // Small delay so user sees 100%
        await Future.delayed(const Duration(milliseconds: 500));
        if (mounted) Navigator.pop(context);
        await UpdateService._openDownloadedFile(filePath);
      } else {
        setState(() {
          _error = "Download failed. Try again or download manually.";
          _downloading = false;
        });
      }
    } catch (e) {
      setState(() {
        _error = "Download error: $e";
        _downloading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return BackdropFilter(
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
              color: AppTheme.current.accent.withValues(alpha: 0.3),
              width: 1.5,
            ),
            boxShadow: [
              BoxShadow(
                color: AppTheme.current.accent.withValues(alpha: 0.1),
                blurRadius: 24,
                offset: const Offset(0, 8),
              )
            ],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Header
              Row(
                children: [
                  Container(
                    padding: EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: AppTheme.current.accent.withValues(alpha: 0.1),
                      shape: BoxShape.circle,
                    ),
                    child: Icon(
                      Icons.rocket_launch_rounded,
                      color: AppTheme.current.accent,
                      size: 28,
                    ),
                  ),
                  SizedBox(width: 16),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          "NEW UPDATE AVAILABLE",
                          style: TextStyle(
                            color: AppTheme.current.accent,
                            fontSize: 11,
                            fontWeight: FontWeight.bold,
                            letterSpacing: 1.5,
                          ),
                        ),
                        SizedBox(height: 2),
                        Text(
                          "Version v${widget.latestVersion}",
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

              // Changelog
              Text(
                "What's New:",
                style: TextStyle(
                  color: AppTheme.current.text.withValues(alpha: 0.7),
                  fontSize: 13,
                  fontWeight: FontWeight.bold,
                ),
              ),
              SizedBox(height: 8),
              if (widget.changelog.isEmpty)
                Text(
                  "Performance updates and bug fixes.",
                  style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 13),
                )
              else
                Container(
                  constraints: const BoxConstraints(maxHeight: 150),
                  child: ListView.builder(
                    shrinkWrap: true,
                    itemCount: widget.changelog.length,
                    itemBuilder: (lCtx, idx) {
                      return Padding(
                        padding: EdgeInsets.only(bottom: 6),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              "\u2022 ",
                              style: TextStyle(
                                color: AppTheme.current.accent,
                                fontWeight: FontWeight.bold,
                                fontSize: 14,
                              ),
                            ),
                            Expanded(
                              child: Text(
                                widget.changelog[idx],
                                style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.6), fontSize: 13),
                              ),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),

              // Error message
              if (_error != null) ...[
                SizedBox(height: 12),
                Container(
                  padding: EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: Colors.redAccent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: Colors.redAccent.withValues(alpha: 0.2)),
                  ),
                  child: Text(_error!, style: TextStyle(color: Colors.redAccent, fontSize: 12)),
                ),
              ],

              SizedBox(height: 24),

              // Download progress bar
              if (_downloading) ...[
                ClipRRect(
                  borderRadius: BorderRadius.circular(6),
                  child: LinearProgressIndicator(
                    value: _progress,
                    backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                    valueColor: AlwaysStoppedAnimation(AppTheme.current.accent),
                    minHeight: 8,
                  ),
                ),
                SizedBox(height: 8),
                Center(
                  child: Text(
                    _progress < 1.0
                        ? "Downloading ${widget.platformLabel}... ${(_progress * 100).toStringAsFixed(0)}%"
                        : "Download complete. Opening...",
                    style: TextStyle(
                      color: AppTheme.current.text.withValues(alpha: 0.6),
                      fontSize: 12,
                    ),
                  ),
                ),
                SizedBox(height: 16),
              ],

              // Buttons
              if (!_downloading)
                Row(
                  children: [
                    Expanded(
                      child: OutlinedButton(
                        style: OutlinedButton.styleFrom(
                          side: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                          padding: EdgeInsets.symmetric(vertical: 14),
                        ),
                        onPressed: () => Navigator.pop(context),
                        child: Text("LATER", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontWeight: FontWeight.bold, fontSize: 13)),
                      ),
                    ),
                    SizedBox(width: 12),
                    Expanded(
                      child: ElevatedButton(
                        style: ElevatedButton.styleFrom(
                          backgroundColor: AppTheme.current.accent,
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                          padding: EdgeInsets.symmetric(vertical: 14),
                        ),
                        onPressed: widget.hasDownload
                            ? _startDownload
                            : () async {
                                // Fallback: open GitHub releases page
                                final uri = Uri.parse('https://github.com/d3v6k/introvert/releases/latest');
                                if (await canLaunchUrl(uri)) {
                                  await launchUrl(uri, mode: LaunchMode.externalApplication);
                                }
                              },
                        child: Text(
                          widget.hasDownload ? "DOWNLOAD ${widget.platformLabel.toUpperCase()}" : "VIEW ON GITHUB",
                          style: TextStyle(
                            color: Colors.black,
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
    );
  }
}
