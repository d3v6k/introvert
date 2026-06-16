import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:open_file/open_file.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';


class DriveTab extends StatefulWidget {
  const DriveTab({super.key});

  @override
  State<DriveTab> createState() => _DriveTabState();
}

class _DriveTabState extends State<DriveTab> with AutomaticKeepAliveClientMixin {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _searchController = TextEditingController();
  List<dynamic> _allFiles = [];
  List<dynamic> _filteredFiles = [];
  int _seedingCount = 0;
  int _sovereignRemaining = 0;
  bool _isLoading = true;
  Timer? _refreshTimer;
  Map<String, FileTransferProgress> _activeTransfers = {};
  Map<String, dynamic>? _swarmStats;
  StreamSubscription? _networkSubscription;
  StreamSubscription? _swarmSubscription;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _initPath();
    _loadFiles();
    _startListeners();
    _refreshTimer = Timer.periodic(Duration(seconds: 30), (_) => _loadFiles()); // 30s (was 5s) — battery optimization
  }

  Future<void> _initPath() async {
    // Path initialization - ready for future use
  }

  @override
  void dispose() {
    _refreshTimer?.cancel();
    _networkSubscription?.cancel();
    _swarmSubscription?.cancel();
    _searchController.dispose();
    super.dispose();
  }

  void _startListeners() {
    _swarmSubscription = _client.swarmStatsStream.listen((stats) {
      if (mounted) {
        setState(() => _swarmStats = stats);
      }
    });

    _networkSubscription = _client.networkStream.listen((event) {
      if (event.type == 12) {
        try {
          final progress = FileTransferProgress.fromJson(json.decode(utf8.decode(event.data)));
          if (mounted) {
            setState(() {
              _activeTransfers[progress.transferId] = progress;
              if (progress.isComplete) {
                _loadFiles();
                Future.delayed(Duration(seconds: 2), () {
                  if (mounted) setState(() => _activeTransfers.remove(progress.transferId));
                });
              }
            });
          }
        } catch (_) {}
      }
    });
  }

  Future<void> _loadFiles() async {
    try {
      _client.requestSwarmStats();
      final files = _client.driveGetAll();
      
      int seeding = 0;
      int used = 0;
      for (var f in files) {
        final path = _client.resolveSandboxPath(f['local_path']?.toString()) ?? "";
        final size = f['total_size'] as int? ?? 0;
        if (path.isNotEmpty && File(path).existsSync()) {
          seeding++;
          used += size;
        }
      }

      const int sovereignLimit = 1024 * 1024 * 1024; // 1GB
      final remaining = (sovereignLimit - used).clamp(0, sovereignLimit);

      if (mounted) {
        setState(() {
          _allFiles = files;
          _seedingCount = seeding;
          _sovereignRemaining = remaining;
          _filterFiles(_searchController.text);
          _isLoading = false;
        });
      }
    } catch (e) {
      debugPrint("Error loading drive files: $e");
      if (mounted) setState(() => _isLoading = false);
    }
  }

  void _filterFiles(String query) {
    if (query.isEmpty) {
      _filteredFiles = List.from(_allFiles);
    } else {
      _filteredFiles = _allFiles.where((f) {
        final name = f['filename']?.toString().toLowerCase() ?? "";
        return name.contains(query.toLowerCase());
      }).toList();
    }
  }

  Future<void> _pickAndUpload() async {
    final result = await FilePicker.platform.pickFiles();
    if (result != null && result.files.single.path != null) {
      final file = File(result.files.single.path!);
      final size = await file.length();
      final filename = result.files.single.name;
      
      // In a real app, we might move the file to our encrypted storage first.
      // For this prototype, we'll just add it to the Drive tracking.
      _client.driveAddFile(filename, "manual_${DateTime.now().millisecondsSinceEpoch}", "application/octet-stream", size, file.path);
      _loadFiles();
    }
  }

  void _openFile(String name, String path) async {
    final result = await OpenFile.open(path);
    if (!mounted) return;
    if (result.type != ResultType.done) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text("Cannot open file: ${result.message}"),
          backgroundColor: Colors.orange,
        ),
      );
    }
  }

  void _forwardFileToChat(String name, String hash, String mime, int size, String path) {
    final contacts = _client.getContacts();
    final groups = _client.getAllGroups();

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (context) {
        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: Text(
                  "Forward \"$name\"",
                  style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold),
                ),
              ),
              Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1), height: 1),
              if (contacts.isEmpty && groups.isEmpty)
                Padding(
                  padding: const EdgeInsets.all(32.0),
                  child: Text("No active chats available.", style: TextStyle(color: AppTheme.current.mutedText)),
                )
              else
                Expanded(
                  child: ListView(
                    children: [
                      if (contacts.isNotEmpty) ...[
                        Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Text("DIRECT CHATS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold)),
                        ),
                        ...contacts.map((c) {
                          return Material(color: Colors.transparent, child: ListTile(
                            leading: Icon(Icons.person, color: AppTheme.current.mutedText),
                            title: Text(c['alias']?.toString().isNotEmpty == true ? c['alias'] : (c['global_name'] ?? c['peer_id']), style: TextStyle(color: AppTheme.current.text)),
                            onTap: () {
                              _client.sendFile(c['peer_id'], path);
                              Navigator.pop(context);
                              ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("File forwarded to direct chat.", style: TextStyle(color: AppTheme.current.accent))));
                            },
                          ));
                        }),
                        ],
                        if (groups.isNotEmpty) ...[
                        Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Text("MESH ROOMS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold)),
                        ),
                        ...groups.map((g) {
                          return Material(color: Colors.transparent, child: ListTile(
                            leading: Icon(Icons.group, color: AppTheme.current.mutedText),
                            title: Text(g[1].toString(), style: TextStyle(color: AppTheme.current.text)),
                            onTap: () {
                              final transferId = "gft_${hash}_${DateTime.now().millisecondsSinceEpoch}";
                              // 1. Register Seeder
                              _client.registerSeeder(transferId, path, hash, size, g[0]);

                              // 2. Broadcast Manifest
                              final manifest = '{"transfer_id":"$transferId","peer_id":"${_client.localPeerId}","filename":"$name","mime_type":"$mime","total_size":$size,"file_hash":"$hash","is_relayed":true,"group_id":"${g[0]}"}';
                              _client.sendGroupMessage(g[0], "[FILE]:$manifest");

                              Navigator.pop(context);
                              ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("File forwarded to group.", style: TextStyle(color: AppTheme.current.accent))));
                            },
                          ));
                        }),

                      ],
                    ],
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  void _pullFromMesh(String name, String hash, String mime, int size) {
    // Generate a new transfer ID for this pull
    final tid = "pull_${hash.substring(0, 8)}_${DateTime.now().millisecondsSinceEpoch}";
    _client.startPull("", tid, name, mime, hash, size, true);
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text("Requesting '$name' from mesh swarm...")),
    );
  }

  String _formatBytes(int bytes) {
    if (bytes <= 0) return "0 B";
    const suffixes = ["B", "KB", "MB", "GB", "TB"];
    var i = (math.log(bytes) / math.log(1024)).floor();
    return '${(bytes / math.pow(1024, i)).toStringAsFixed(1)} ${suffixes[i]}';
  }

  ({IconData icon, Color color}) _fileIcon(String name) {
    final ext = name.split('.').last.toLowerCase();
    switch (ext) {
      case 'png': case 'jpg': case 'jpeg': case 'gif': case 'webp':
        return (icon: Icons.image_rounded, color: Colors.blueAccent);
      case 'mp4': case 'mov': case 'avi': case 'mkv':
        return (icon: Icons.videocam_rounded, color: Colors.purpleAccent);
      case 'mp3': case 'wav': case 'm4a': case 'flac':
        return (icon: Icons.audiotrack_rounded, color: Colors.orangeAccent);
      case 'pdf':
        return (icon: Icons.picture_as_pdf_rounded, color: Colors.redAccent);
      case 'zip': case 'rar': case '7z': case 'tar': case 'gz':
        return (icon: Icons.folder_zip_rounded, color: Colors.amberAccent);
      case 'txt': case 'doc': case 'docx':
        return (icon: Icons.description_rounded, color: Colors.blueGrey);
      default:
        return (icon: Icons.insert_drive_file_rounded, color: AppTheme.current.accent);
    }
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Column(
        children: [
          _buildMeshCapacityCard(),
          Padding(
            padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: TextField(
              controller: _searchController,
              style: TextStyle(color: AppTheme.current.text, fontSize: 13),
              onChanged: _filterFiles,
              decoration: InputDecoration(
                hintText: "Search Sovereign Drive...",
                hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                prefixIcon: Icon(Icons.search, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                filled: true,
                fillColor: Colors.black26,
                contentPadding: EdgeInsets.zero,
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
              ),
            ),
          ),
          Expanded(
            child: _isLoading 
                ? Center(child: CircularProgressIndicator(color: AppTheme.current.accent))
                : _allFiles.isEmpty 
                    ? _buildEmptyState()
                    : _buildFileList(),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: _pickAndUpload,
        backgroundColor: AppTheme.current.accent,
        child: Icon(Icons.add, color: Colors.black),
      ),
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.cloud_off_rounded, size: 64, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          SizedBox(height: 16),
          Text(
            "Sovereign Drive is Empty",
            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 16, fontWeight: FontWeight.bold),
          ),
          SizedBox(height: 8),
          Text(
            "Upload files to persist them in your mesh.",
            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 12),
          ),
        ],
      ),
    );
  }

  Widget _buildFileList() {
    // Group files by folder
    final Map<String, List<Map<String, dynamic>>> groups = {};
    for (final f in _filteredFiles) {
      if (f is! Map<String, dynamic>) continue;
      final localPath = _client.resolveSandboxPath(f['local_path']?.toString()) ?? "";
      String folderName = "Sovereign Root";
      if (localPath.isNotEmpty) {
        final parts = localPath.split('/');
        if (parts.length > 2) {
          folderName = parts[parts.length - 2].replaceAll('_', ' ');
        }
      }
      groups.putIfAbsent(folderName, () => []).add(f);
    }

    final sortedFolders = groups.keys.toList()..sort((a, b) {
      if (a == "Sovereign Root") return -1;
      if (b == "Sovereign Root") return 1;
      return a.compareTo(b);
    });

    return ListView.builder(
      itemCount: sortedFolders.length,
      padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      itemBuilder: (context, folderIdx) {
        final folderName = sortedFolders[folderIdx];
        final folderFiles = groups[folderName]!;

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Row(
                children: [
                  Icon(Icons.folder_open_rounded, color: AppTheme.current.accent.withValues(alpha: 0.5), size: 16),
                  SizedBox(width: 8),
                  Text(
                    folderName.toUpperCase(),
                    style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2),
                  ),
                ],
              ),
            ),
            ...folderFiles.map((f) {
              final name = f['filename']?.toString() ?? "Unknown";
              final hash = f['file_hash']?.toString() ?? "";
              final size = f['total_size'] as int? ?? 0;
              final isBackedUp = f['is_backed_up'] == true;
              final localPath = _client.resolveSandboxPath(f['local_path']?.toString()) ?? "";
              final (:icon, :color) = _fileIcon(name);
              
              bool exists = false;
              if (localPath.isNotEmpty) {
                exists = File(localPath).existsSync();
              }

              FileTransferProgress? active;
              for (var t in _activeTransfers.values) {
                if (t.transferId.contains(hash) && !t.isComplete) {
                  active = t;
                  break;
                }
              }

              return Material(
                color: Colors.transparent,
                child: InkWell(
                  onTap: () {
                    if (exists) {
                      _openFile(name, localPath);
                    } else if (active == null) {
                      _pullFromMesh(name, hash, f['mime_type']?.toString() ?? 'application/octet-stream', size);
                    }
                  },
                  borderRadius: BorderRadius.circular(12),
                  splashColor: AppTheme.current.accent.withValues(alpha: 0.08),
                  child: ListTile(
                    leading: Container(
                      width: 44,
                      height: 44,
                      decoration: BoxDecoration(
                        color: color.withValues(alpha: 0.08),
                        borderRadius: BorderRadius.circular(10),
                        border: Border.all(color: color.withValues(alpha: 0.2), width: 1),
                      ),
                      child: active != null
                        ? Padding(
                            padding: EdgeInsets.all(10),
                            child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent),
                          )
                        : Icon(icon, color: color, size: 22),
                    ),
                    title: Text(
                      name,
                      style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold),
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Text(
                              _formatBytes(size),
                              style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 11),
                            ),
                            SizedBox(width: 6),
                            Container(
                              padding: EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                              decoration: BoxDecoration(
                                color: exists
                                    ? (isBackedUp ? AppTheme.current.accent.withValues(alpha: 0.1) : AppTheme.current.text.withValues(alpha: 0.05))
                                    : Colors.orangeAccent.withValues(alpha: 0.1),
                                borderRadius: BorderRadius.circular(4),
                              ),
                              child: Text(
                                exists 
                                  ? (isBackedUp ? 'MESH SECURED' : 'LOCAL ONLY')
                                  : (active != null ? 'PULLING...' : 'REMOTE ONLY'),
                                style: TextStyle(
                                  color: exists 
                                    ? (isBackedUp ? AppTheme.current.accent : AppTheme.current.mutedText.withValues(alpha: 0.5))
                                    : (active != null ? AppTheme.current.accent : Colors.orangeAccent),
                                  fontSize: 9,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 0.5,
                                ),
                              ),
                            ),
                          ],
                        ),
                        if (active != null)
                          Padding(
                            padding: EdgeInsets.only(top: 6),
                            child: ClipRRect(
                              borderRadius: BorderRadius.circular(2),
                              child: LinearProgressIndicator(
                                value: active.progress,
                                minHeight: 2,
                                backgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                                valueColor: AlwaysStoppedAnimation<Color>(AppTheme.current.accent),
                              ),
                            ),
                          ),
                      ],
                    ),
                    trailing: exists 
                      ? PopupMenuButton<String>(
                          icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                          color: AppTheme.current.surface,
                          onSelected: (val) {
                            if (val == 'open') {
                              _openFile(name, localPath);
                            } else if (val == 'delete') {
                              try {
                                final fileToDelete = File(localPath);
                                if (fileToDelete.existsSync()) fileToDelete.deleteSync();
                              } catch (e) {
                                debugPrint("Error deleting physical file: $e");
                              }
                              _client.driveDelete(hash);
                              _loadFiles();
                            } else if (val == 'forward') {
                              _forwardFileToChat(name, hash, f['mime_type']?.toString() ?? 'application/octet-stream', size, localPath);
                            }
                          },
                          itemBuilder: (ctx) => [
                            PopupMenuItem(
                              value: 'open',
                              child: Row(
                                children: [
                                  Icon(Icons.open_in_new_rounded, size: 18, color: AppTheme.current.text.withValues(alpha: 0.7)),
                                  SizedBox(width: 10),
                                  Text("Open", style: TextStyle(color: AppTheme.current.text)),
                                ],
                              ),
                            ),
                            PopupMenuItem(
                              value: 'forward',
                              child: Row(
                                children: [
                                  Icon(Icons.forward_to_inbox_rounded, size: 18, color: AppTheme.current.text.withValues(alpha: 0.7)),
                                  SizedBox(width: 10),
                                  Text("Forward to Chat", style: TextStyle(color: AppTheme.current.text)),
                                ],
                              ),
                            ),
                            PopupMenuItem(
                              value: 'delete',
                              child: Row(
                                children: [
                                  Icon(Icons.delete_outline_rounded, size: 18, color: Colors.redAccent.withValues(alpha: 0.7)),
                                  SizedBox(width: 10),
                                  Text("Delete", style: TextStyle(color: Colors.redAccent)),
                                ],
                              ),
                            ),
                          ],

                        )
                      : null,
                  ),
                ),
              );
            }).toList(),
            SizedBox(height: 16),
          ],
        );
      },
    );
  }

  Widget _buildMeshCapacityCard() {
    return Container(
      width: double.infinity,
      margin: EdgeInsets.all(16),
      padding: EdgeInsets.all(20),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [AppTheme.current.accent.withValues(alpha: 0.15), Colors.transparent],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text("MESH CAPACITY", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.5, fontWeight: FontWeight.bold)),
                  SizedBox(height: 4),
                  Text("Sovereign Storage", style: TextStyle(color: AppTheme.current.text, fontSize: 18, fontWeight: FontWeight.w900)),
                ],
              ),
              Container(
                padding: EdgeInsets.all(8),
                decoration: BoxDecoration(color: AppTheme.current.accent.withValues(alpha: 0.1), shape: BoxShape.circle),
                child: Icon(Icons.storage_rounded, color: AppTheme.current.accent, size: 24),
              ),
            ],
          ),
          SizedBox(height: 20),
          Row(
            children: [
              _buildStatItem("COLLECTIVE", _swarmStats != null ? "${(_swarmStats!['collective_capacity_gb'] as num? ?? 0)} GB" : "---", Icons.grain),
              SizedBox(width: 24),
              _buildStatItem("LOCAL", _formatBytes(_sovereignRemaining), Icons.sd_storage),
              SizedBox(width: 24),
              _buildStatItem("SEEDING", "$_seedingCount", Icons.upload_rounded),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildStatItem(String label, String value, IconData icon) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(icon, size: 10, color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
            SizedBox(width: 4),
            Text(label, style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 9, fontWeight: FontWeight.bold)),
          ],
        ),
        SizedBox(height: 4),
        Text(value, style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold, fontFamily: 'monospace')),
      ],
    );
  }
}
