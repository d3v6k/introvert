import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:open_file/open_file.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';
import '../../blueprint_ui.dart';

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
  String _searchQuery = '';
  bool _isLoading = true;
  bool _isDisposing = false;
  bool _isGridView = false;
  bool _isMultiSelect = false;
  Set<String> _selectedHashes = {};
  Timer? _refreshTimer;
  Map<String, FileTransferProgress> _activeTransfers = {};
  Map<String, dynamic>? _swarmStats;
  StreamSubscription? _networkSubscription;
  StreamSubscription? _swarmSubscription;

  // Folder state
  String _currentFolder = '';
  final List<String> _breadcrumb = ['Drive'];
  Map<String, List<dynamic>> _folderGroups = {};

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _initDrive();
    _startListeners();
    _refreshTimer = Timer.periodic(Duration(seconds: 60), (_) => _loadFiles());
  }

  Future<void> _initDrive() async {
    await _seedIntrovertExplained();
    _loadFiles();
  }

  Future<void> _seedIntrovertExplained() async {
    try {
      final existing = _client.driveGetAll();
      final hasExplained = existing.any((f) =>
          (f['folder']?.toString() ?? '') == 'Introvert Explained');

      // Fix existing entries that were added without the correct folder
      for (final f in existing) {
        final path = f['local_path']?.toString() ?? '';
        final folder = f['folder']?.toString() ?? '';
        if (path.contains('Introvert Explained') && folder != 'Introvert Explained') {
          _client.driveUpdateFolder(f['file_hash']?.toString() ?? '', 'Introvert Explained');
        }
      }

      if (hasExplained) return;

      final appDir = await getApplicationDocumentsDirectory();
      final explainedDir = Directory('${appDir.path}/Introvert Explained');
      if (!explainedDir.existsSync()) await explainedDir.create(recursive: true);

      const assets = [
        'assets/images/introvert_explained/Account_Prestige_and_Point_Telemetry.png',
        'assets/images/introvert_explained/Off-Grid_Messaging_Delivery_Manual.png',
        'assets/images/introvert_explained/Self-Healing_Swarm_Resilience_Guide.png',
        'assets/images/introvert_explained/Sovereign_Login_and_Security_Anatomy.png',
      ];

      for (final assetPath in assets) {
        final filename = assetPath.split('/').last;
        final destPath = '${explainedDir.path}/$filename';
        final destFile = File(destPath);
        if (!destFile.existsSync()) {
          final bytes = await rootBundle.load(assetPath);
          await destFile.writeAsBytes(bytes.buffer.asUint8List());
        }
        final fileBytes = await destFile.readAsBytes();
        final hash = sha256.convert(fileBytes).toString();
        _client.driveAddFileWithFolder(filename, hash, 'image/png', fileBytes.length, destPath, 'Introvert Explained');
      }
    } catch (e) {
      debugPrint('[Drive] Failed to seed Introvert Explained: $e');
    }
  }

  void _startListeners() {
    // Drive tab doesn't need network listeners
  }

  void _loadFiles() {
    if (_isDisposing) return;
    try {
      final files = _client.driveGetAll();
      if (mounted) {
        setState(() {
          _allFiles = files;
          _applyFilter();
          _buildFolderGroups();
          _isLoading = false;
        });
      }
    } catch (e) {
      debugPrint('[Drive] Load error: $e');
    }
  }

  void _applyFilter() {
    if (_searchQuery.isEmpty) {
      _filteredFiles = List.from(_allFiles);
    } else {
      final q = _searchQuery.toLowerCase();
      _filteredFiles = _allFiles.where((f) =>
          (f['filename']?.toString().toLowerCase() ?? '').contains(q) ||
          (f['folder']?.toString().toLowerCase() ?? '').contains(q)).toList();
    }
  }

  void _buildFolderGroups() {
    _folderGroups.clear();
    for (final file in _filteredFiles) {
      String folder = file['folder']?.toString() ?? '';
      if (folder.isEmpty) folder = 'Uploads';
      _folderGroups.putIfAbsent(folder, () => []).add(file);
    }
    // Sort: Introvert Explained first, then alphabetical
    final sorted = _folderGroups.keys.toList()..sort((a, b) {
      if (a == 'Introvert Explained') return -1;
      if (b == 'Introvert Explained') return 1;
      return a.compareTo(b);
    });
    final newGroups = <String, List<dynamic>>{};
    for (final k in sorted) {
      newGroups[k] = _folderGroups[k]!;
    }
    _folderGroups = newGroups;
  }

  String _formatFileSize(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    if (bytes < 1024 * 1024 * 1024) return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
    return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
  }

  IconData _getFileIcon(String mime) {
    if (mime.startsWith('image/')) return Icons.image_rounded;
    if (mime.startsWith('video/')) return Icons.videocam_rounded;
    if (mime.startsWith('audio/')) return Icons.audiotrack_rounded;
    if (mime.contains('pdf')) return Icons.picture_as_pdf_rounded;
    if (mime.contains('zip') || mime.contains('archive')) return Icons.folder_zip_rounded;
    return Icons.insert_drive_file_rounded;
  }

  Color _getFileColor(String mime) {
    if (mime.startsWith('image/')) return Colors.greenAccent;
    if (mime.startsWith('video/')) return Colors.redAccent;
    if (mime.startsWith('audio/')) return Colors.orangeAccent;
    if (mime.contains('pdf')) return Colors.redAccent;
    return Colors.blueAccent;
  }

  void _navigateToFolder(String folder) {
    setState(() {
      _currentFolder = folder;
      _breadcrumb.add(folder);
      _filteredFiles = _folderGroups[folder] ?? [];
    });
  }

  void _navigateBack() {
    if (_breadcrumb.length > 1) {
      setState(() {
        _breadcrumb.removeLast();
        _currentFolder = _breadcrumb.last;
        if (_currentFolder == 'Drive') {
          _filteredFiles = List.from(_allFiles);
          _buildFolderGroups();
        } else {
          _filteredFiles = _folderGroups[_currentFolder] ?? [];
        }
      });
    }
  }

  void _toggleMultiSelect() {
    setState(() {
      _isMultiSelect = !_isMultiSelect;
      if (!_isMultiSelect) _selectedHashes.clear();
    });
  }

  void _toggleSelection(String hash) {
    setState(() {
      if (_selectedHashes.contains(hash)) {
        _selectedHashes.remove(hash);
      } else {
        _selectedHashes.add(hash);
      }
    });
  }

  void _selectAll() {
    setState(() {
      _selectedHashes = _filteredFiles.map((f) => f['file_hash']?.toString() ?? '').toSet();
    });
  }

  void _deleteSelected() {
    if (_selectedHashes.isEmpty) return;
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Delete ${_selectedHashes.length} files?'),
        content: Text('This action cannot be undone.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text('Cancel')),
          TextButton(
            onPressed: () {
              for (final hash in _selectedHashes) {
                _client.driveDelete(hash);
              }
              Navigator.pop(ctx);
              _toggleMultiSelect();
              _loadFiles();
            },
            child: Text('Delete', style: TextStyle(color: Colors.redAccent)),
          ),
        ],
      ),
    );
  }

  void _moveSelectedToFolder() {
    if (_selectedHashes.isEmpty) return;
    final folders = _folderGroups.keys.where((f) => f != _currentFolder).toList();
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Move to folder'),
        content: SizedBox(
          width: double.maxFinite,
          child: ListView.builder(
            shrinkWrap: true,
            itemCount: folders.length,
            itemBuilder: (ctx, i) => ListTile(
              leading: Icon(Icons.folder_rounded, color: Colors.amberAccent),
              title: Text(folders[i]),
              onTap: () {
                for (final hash in _selectedHashes) {
                  _client.driveUpdateFolder(hash, folders[i]);
                }
                Navigator.pop(ctx);
                _toggleMultiSelect();
                _loadFiles();
              },
            ),
          ),
        ),
      ),
    );
  }

  void _shareSelectedFiles() {
    if (_selectedHashes.isEmpty) return;
    final paths = _allFiles
        .where((f) => _selectedHashes.contains(f['file_hash']))
        .map((f) => f['local_path']?.toString() ?? '')
        .where((p) => p.isNotEmpty)
        .toList();
    if (paths.isNotEmpty) {
      Share.shareXFiles(paths.map((p) => XFile(p)).toList());
    }
  }

  Widget _buildBreadcrumb() {
    return Container(
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: Row(
        children: [
          for (int i = 0; i < _breadcrumb.length; i++) ...[
            if (i > 0) Icon(Icons.chevron_right_rounded, size: 16, color: AppTheme.current.mutedText),
            GestureDetector(
              onTap: () {
                if (i < _breadcrumb.length - 1) {
                  setState(() {
                    _breadcrumb.removeRange(i + 1, _breadcrumb.length);
                    _currentFolder = _breadcrumb.last;
                    if (_currentFolder == 'Drive') {
                      _filteredFiles = List.from(_allFiles);
                      _buildFolderGroups();
                    } else {
                      _filteredFiles = _folderGroups[_currentFolder] ?? [];
                    }
                  });
                }
              },
              child: Text(
                _breadcrumb[i],
                style: TextStyle(
                  color: i == _breadcrumb.length - 1 ? AppTheme.current.accent : AppTheme.current.mutedText,
                  fontWeight: i == _breadcrumb.length - 1 ? FontWeight.w600 : FontWeight.normal,
                  fontSize: 13,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildStorageBar() {
    final totalSize = _allFiles.fold<int>(0, (sum, f) => sum + ((f['total_size'] as int?) ?? 0));
    return Container(
      margin: EdgeInsets.symmetric(horizontal: 16, vertical: 4),
      padding: EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AppTheme.current.surface.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text('Storage', style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText)),
              Text('${_formatFileSize(totalSize)} used', style: TextStyle(fontSize: 12, color: AppTheme.current.accent)),
            ],
          ),
          SizedBox(height: 6),
          LinearProgressIndicator(
            value: (totalSize / (1024 * 1024 * 1024)).clamp(0.0, 1.0),
            backgroundColor: AppTheme.current.surface,
            valueColor: AlwaysStoppedAnimation(AppTheme.current.accent),
          ),
        ],
      ),
    );
  }

  Widget _buildSovereignDriveCard() {
    final totalSize = _allFiles.fold<int>(0, (sum, f) => sum + ((f['total_size'] as int?) ?? 0));
    final usedStr = _formatFileSize(totalSize);
    final limitBytes = 1 * 1024 * 1024 * 1024; // 1 GB
    final percentUsed = (totalSize / limitBytes).clamp(0.0, 1.0);

    return Container(
      margin: EdgeInsets.fromLTRB(16, 8, 16, 4),
      padding: EdgeInsets.all(16),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [
            AppTheme.current.accent.withValues(alpha: 0.1),
            AppTheme.current.accent.withValues(alpha: 0.03),
          ],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.cloud_done_rounded, color: AppTheme.current.accent, size: 24),
              SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'SOVEREIGN DRIVE',
                      style: TextStyle(
                        color: AppTheme.current.accent,
                        fontSize: 10,
                        fontWeight: FontWeight.bold,
                        letterSpacing: 1.2,
                      ),
                    ),
                    SizedBox(height: 2),
                    Text(
                      '${_allFiles.length} files · ${_folderGroups.length} folders',
                      style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10),
                    ),
                  ],
                ),
              ),
              Text(
                '${(percentUsed * 100).toStringAsFixed(0)}%',
                style: TextStyle(
                  color: AppTheme.current.accent,
                  fontSize: 14,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ],
          ),
          SizedBox(height: 12),
          ClipRRect(
            borderRadius: BorderRadius.circular(4),
            child: LinearProgressIndicator(
              value: percentUsed,
              backgroundColor: AppTheme.current.surface,
              valueColor: AlwaysStoppedAnimation(AppTheme.current.accent),
              minHeight: 4,
            ),
          ),
          SizedBox(height: 6),
          Text(
            '$usedStr of 1 GB used',
            style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10),
          ),
        ],
      ),
    );
  }

  Widget _buildFolderView() {
    if (_currentFolder.isNotEmpty && _currentFolder != 'Drive') {
      return _buildFileList(_filteredFiles);
    }

    return ListView(
      children: [
        ..._folderGroups.entries.map((entry) {
          final folderName = entry.key;
          final files = entry.value;
          final totalSize = files.fold<int>(0, (sum, f) => sum + ((f['total_size'] as int?) ?? 0));
          final isExplained = folderName == 'Introvert Explained';

          return Container(
            margin: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
            decoration: BoxDecoration(
              color: isExplained
                  ? AppTheme.current.accent.withValues(alpha: 0.08)
                  : AppTheme.current.surface.withValues(alpha: 0.3),
              borderRadius: BorderRadius.circular(12),
              border: isExplained
                  ? Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3))
                  : null,
            ),
            child: Theme(
              data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
              child: ExpansionTile(
                initiallyExpanded: isExplained,
                leading: Icon(
                  isExplained ? Icons.auto_stories_rounded : Icons.folder_rounded,
                  color: isExplained ? AppTheme.current.accent : Colors.amberAccent,
                ),
                title: Text(folderName, style: TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
                subtitle: Text('${files.length} files • ${_formatFileSize(totalSize)}',
                    style: TextStyle(fontSize: 11, color: AppTheme.current.mutedText)),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (!isExplained)
                      IconButton(
                        icon: Icon(Icons.share_rounded, size: 18, color: AppTheme.current.mutedText),
                        onPressed: () => _shareFolder(folderName, files),
                        tooltip: 'Share folder',
                      ),
                    Icon(Icons.expand_more_rounded, color: AppTheme.current.mutedText),
                  ],
                ),
                children: files.map((file) => _buildFileTile(file)).toList(),
              ),
            ),
          );
        }),
      ],
    );
  }

  Widget _buildFileList(List<dynamic> files) {
    if (_isGridView) {
      return GridView.builder(
        padding: EdgeInsets.all(12),
        gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: 3,
          childAspectRatio: 0.8,
          crossAxisSpacing: 8,
          mainAxisSpacing: 8,
        ),
        itemCount: files.length,
        itemBuilder: (ctx, i) => _buildFileGridItem(files[i]),
      );
    }
    return ListView.builder(
      padding: EdgeInsets.all(8),
      itemCount: files.length,
      itemBuilder: (ctx, i) => _buildFileTile(files[i]),
    );
  }

  Widget _buildFileGridItem(dynamic file) {
    final hash = file['file_hash']?.toString() ?? '';
    final mime = file['mime_type']?.toString() ?? '';
    final size = (file['total_size'] as int?) ?? 0;
    final isSelected = _selectedHashes.contains(hash);

    return GestureDetector(
      onTap: () {
        if (_isMultiSelect) {
          _toggleSelection(hash);
        } else {
          _openFile(file);
        }
      },
      onLongPress: () {
        if (!_isMultiSelect) {
          _toggleMultiSelect();
          _toggleSelection(hash);
        }
      },
      child: Container(
        decoration: BoxDecoration(
          color: isSelected
              ? AppTheme.current.accent.withValues(alpha: 0.15)
              : AppTheme.current.surface.withValues(alpha: 0.3),
          borderRadius: BorderRadius.circular(12),
          border: isSelected
              ? Border.all(color: AppTheme.current.accent, width: 2)
              : null,
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            if (_isMultiSelect)
              Align(
                alignment: Alignment.topRight,
                child: Padding(
                  padding: EdgeInsets.all(4),
                  child: Icon(
                    isSelected ? Icons.check_circle_rounded : Icons.circle_outlined,
                    size: 20,
                    color: isSelected ? AppTheme.current.accent : AppTheme.current.mutedText,
                  ),
                ),
              ),
            Icon(_getFileIcon(mime), size: 36, color: _getFileColor(mime)),
            SizedBox(height: 8),
            Padding(
              padding: EdgeInsets.symmetric(horizontal: 8),
              child: Text(
                file['filename']?.toString() ?? '',
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 11),
              ),
            ),
            SizedBox(height: 4),
            Text(_formatFileSize(size), style: TextStyle(fontSize: 10, color: AppTheme.current.mutedText)),
          ],
        ),
      ),
    );
  }

  Widget _buildFileTile(dynamic file) {
    final hash = file['file_hash']?.toString() ?? '';
    final mime = file['mime_type']?.toString() ?? '';
    final size = (file['total_size'] as int?) ?? 0;
    final isSelected = _selectedHashes.contains(hash);

    return ListTile(
      leading: _isMultiSelect
          ? Icon(
              isSelected ? Icons.check_circle_rounded : Icons.circle_outlined,
              color: isSelected ? AppTheme.current.accent : AppTheme.current.mutedText,
            )
          : Icon(_getFileIcon(mime), color: _getFileColor(mime)),
      title: Text(file['filename']?.toString() ?? '', style: TextStyle(fontSize: 13)),
      subtitle: Text(_formatFileSize(size), style: TextStyle(fontSize: 11, color: AppTheme.current.mutedText)),
      trailing: _isMultiSelect ? null : PopupMenuButton(
        icon: Icon(Icons.more_vert_rounded, size: 18, color: AppTheme.current.mutedText),
        itemBuilder: (ctx) => [
          PopupMenuItem(value: 'open', child: Text('Open')),
          PopupMenuItem(value: 'move', child: Text('Move to folder')),
          PopupMenuItem(value: 'share', child: Text('Share')),
          PopupMenuItem(value: 'delete', child: Text('Delete', style: TextStyle(color: Colors.redAccent))),
        ],
        onSelected: (action) => _handleFileAction(action, file),
      ),
      onTap: () {
        if (_isMultiSelect) {
          _toggleSelection(hash);
        } else {
          _openFile(file);
        }
      },
      onLongPress: () {
        if (!_isMultiSelect) {
          _toggleMultiSelect();
          _toggleSelection(hash);
        }
      },
    );
  }

  void _openFile(dynamic file) {
    final path = file['local_path']?.toString() ?? '';
    if (path.isNotEmpty && File(path).existsSync()) {
      OpenFile.open(path);
    } else {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('File not found on device')),
      );
    }
  }

  void _handleFileAction(String action, dynamic file) {
    final hash = file['file_hash']?.toString() ?? '';
    switch (action) {
      case 'open':
        _openFile(file);
        break;
      case 'move':
        _selectedHashes = {hash};
        _moveSelectedToFolder();
        break;
      case 'share':
        final path = file['local_path']?.toString() ?? '';
        if (path.isNotEmpty) Share.shareXFiles([XFile(path)]);
        break;
      case 'delete':
        showDialog(
          context: context,
          builder: (ctx) => AlertDialog(
            title: Text('Delete file?'),
            content: Text('This action cannot be undone.'),
            actions: [
              TextButton(onPressed: () => Navigator.pop(ctx), child: Text('Cancel')),
              TextButton(
                onPressed: () {
                  _client.driveDelete(hash);
                  Navigator.pop(ctx);
                  _loadFiles();
                },
                child: Text('Delete', style: TextStyle(color: Colors.redAccent)),
              ),
            ],
          ),
        );
        break;
    }
  }

  void _shareFolder(String folderName, List<dynamic> files) {
    // Share folder manifest to a contact
    final manifest = {
      'type': 'folder_share',
      'folder_name': folderName,
      'files': files.map((f) => {
        'filename': f['filename'],
        'file_hash': f['file_hash'],
        'mime_type': f['mime_type'],
        'total_size': f['total_size'],
      }).toList(),
    };
    Share.share(json.encode(manifest), subject: 'Folder: $folderName');
  }

  void _uploadFile() async {
    try {
      final result = await FilePicker.platform.pickFiles(allowMultiple: true);
      if (result == null || result.files.isEmpty) return;

      final appDir = await getApplicationDocumentsDirectory();
      final uploadDir = Directory('${appDir.path}/Uploads');
      if (!uploadDir.existsSync()) await uploadDir.create(recursive: true);

      for (final file in result.files) {
        if (file.path == null) continue;
        final srcFile = File(file.path!);
        final destPath = '${uploadDir.path}/${file.name}';
        await srcFile.copy(destPath);
        final bytes = await File(destPath).readAsBytes();
        final hash = sha256.convert(bytes).toString();
        _client.driveAddFile(file.name, hash, file.extension ?? 'application/octet-stream', bytes.length, destPath);
      }
      _loadFiles();
    } catch (e) {
      debugPrint('[Drive] Upload error: $e');
    }
  }

  @override
  void dispose() {
    _isDisposing = true;
    _refreshTimer?.cancel();
    _networkSubscription?.cancel();
    _swarmSubscription?.cancel();
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Column(
      children: [
        // Sovereign Drive header
        _buildSovereignDriveCard(),

        // Search bar
        Padding(
          padding: EdgeInsets.fromLTRB(16, 8, 16, 4),
          child: Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _searchController,
                  decoration: InputDecoration(
                    hintText: 'Search files...',
                    prefixIcon: Icon(Icons.search_rounded, size: 20),
                    border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
                    filled: true,
                    fillColor: AppTheme.current.surface.withValues(alpha: 0.5),
                    contentPadding: EdgeInsets.symmetric(vertical: 8),
                    isDense: true,
                  ),
                  onChanged: (v) {
                    setState(() {
                      _searchQuery = v;
                      _applyFilter();
                      _buildFolderGroups();
                    });
                  },
                ),
              ),
              SizedBox(width: 8),
              IconButton(
                icon: Icon(_isGridView ? Icons.view_list_rounded : Icons.grid_view_rounded, size: 20),
                onPressed: () => setState(() => _isGridView = !_isGridView),
                tooltip: _isGridView ? 'List view' : 'Grid view',
              ),
              IconButton(
                icon: Icon(_isMultiSelect ? Icons.close_rounded : Icons.checklist_rounded, size: 20),
                onPressed: _toggleMultiSelect,
                tooltip: _isMultiSelect ? 'Cancel selection' : 'Select',
              ),
            ],
          ),
        ),

        // Breadcrumb
        _buildBreadcrumb(),

        // Multi-select actions
        if (_isMultiSelect)
          Container(
            padding: EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: Row(
              children: [
                Text('${_selectedHashes.length} selected', style: TextStyle(fontSize: 12, color: AppTheme.current.accent)),
                Spacer(),
                TextButton.icon(
                  onPressed: _selectAll,
                  icon: Icon(Icons.select_all_rounded, size: 16),
                  label: Text('All', style: TextStyle(fontSize: 12)),
                ),
                TextButton.icon(
                  onPressed: _moveSelectedToFolder,
                  icon: Icon(Icons.drive_file_move_rounded, size: 16),
                  label: Text('Move', style: TextStyle(fontSize: 12)),
                ),
                TextButton.icon(
                  onPressed: _shareSelectedFiles,
                  icon: Icon(Icons.share_rounded, size: 16),
                  label: Text('Share', style: TextStyle(fontSize: 12)),
                ),
                TextButton.icon(
                  onPressed: _deleteSelected,
                  icon: Icon(Icons.delete_rounded, size: 16, color: Colors.redAccent),
                  label: Text('Delete', style: TextStyle(fontSize: 12, color: Colors.redAccent)),
                ),
              ],
            ),
          ),

        // Content
        Expanded(
          child: _isLoading
              ? Center(child: CircularProgressIndicator())
              : _currentFolder.isNotEmpty && _currentFolder != 'Drive'
                  ? _buildFileList(_filteredFiles)
                  : _buildFolderView(),
        ),
      ],
    );
  }
}
