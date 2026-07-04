import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:image_picker/image_picker.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uuid/uuid.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';
import '../../blueprint_ui.dart';

class NotesTab extends StatefulWidget {
  const NotesTab({super.key});

  @override
  State<NotesTab> createState() => _NotesTabState();
}

class _NotesTabState extends State<NotesTab> with AutomaticKeepAliveClientMixin {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _searchController = TextEditingController();
  List<dynamic> _notes = [];
  List<dynamic> _filteredNotes = [];
  bool _isLoading = true;
  String _searchQuery = '';

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _loadNotes();
  }

  void _loadNotes() {
    setState(() => _isLoading = true);
    try {
      _notes = _client.notesGetAll();
      _applyFilter();
    } catch (e) {
      debugPrint("Error loading notes: $e");
    }
    setState(() => _isLoading = false);
  }

  void _applyFilter() {
    if (_searchQuery.isEmpty) {
      _filteredNotes = List.from(_notes);
    } else {
      final query = _searchQuery.toLowerCase();
      _filteredNotes = _notes.where((n) {
        final title = (n['title'] as String? ?? '').toLowerCase();
        final content = (n['content'] as String? ?? '').toLowerCase();
        final tags = (n['tags'] as String? ?? '').toLowerCase();
        return title.contains(query) || content.contains(query) || tags.contains(query);
      }).toList();
    }
  }

  String _formatDate(String? dateStr) {
    if (dateStr == null || dateStr.isEmpty) return '';
    try {
      // Parse as UTC (SQLite CURRENT_TIMESTAMP is UTC, Rust appends 'Z')
      final date = DateTime.parse(dateStr.replaceFirst(' ', 'T')).toLocal();
      final now = DateTime.now();
      final diff = now.difference(date);
      if (diff.isNegative) return 'Just now';
      if (diff.inMinutes < 1) return 'Just now';
      if (diff.inHours < 1) return '${diff.inMinutes}m ago';
      if (diff.inDays < 1) return '${diff.inHours}h ago';
      if (diff.inDays == 1) return 'Yesterday';
      if (diff.inDays < 7) return '${diff.inDays}d ago';
      return '${date.day}/${date.month}/${date.year}';
    } catch (_) {
      return dateStr;
    }
  }

  String _getPreview(String? content, [int maxLen = 80]) {
    if (content == null || content.isEmpty) return 'No content';
    final text = content.replaceAll('\n', ' ');
    return text.length > maxLen ? '${text.substring(0, maxLen)}...' : text;
  }

  List<String> _parseTags(String? tagsStr) {
    if (tagsStr == null || tagsStr.isEmpty) return [];
    try {
      return List<String>.from(json.decode(tagsStr));
    } catch (_) {
      return [];
    }
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Scaffold(
      backgroundColor: Colors.transparent,
      extendBody: true,      body: Column(
        children: [
          SizedBox(height: MediaQuery.of(context).padding.top + kToolbarHeight),
          GlassmorphicContainer(
            margin: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
            borderRadius: BorderRadius.circular(16),
            tintAlpha: 0.06,
            borderAlpha: 0.1,
            child: Column(
              children: [
                Row(
                  children: [
                    Icon(Icons.sticky_note_2_rounded, color: AppTheme.current.accent, size: 20),
                    const SizedBox(width: 8),
                    Text('SOVEREIGN NOTES', style: TextStyle(
                      fontSize: 13, fontWeight: FontWeight.bold,
                      color: AppTheme.current.accent, letterSpacing: 1.5,
                    )),
                    const Spacer(),
                    IconButton(
                      onPressed: _showHelpDialog,
                      icon: Icon(Icons.help_outline, color: AppTheme.current.mutedText, size: 20),
                      tooltip: 'Help',
                      padding: EdgeInsets.zero,
                      constraints: BoxConstraints(),
                    ),
                    const SizedBox(width: 8),
                    PopupMenuButton(
                      icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText, size: 20),
                      padding: EdgeInsets.zero,
                      onSelected: (val) {
                        if (val == 'export') _showExportDialog();
                        if (val == 'import') _showImportDialog();
                      },
                      itemBuilder: (_) => [
                        const PopupMenuItem(value: 'export', child: Text('Export Notes')),
                        const PopupMenuItem(value: 'import', child: Text('Import Notes')),
                      ],
                    ),
                  ],
                ),
                SizedBox(height: 8),
                TextField(
                  controller: _searchController,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                  onChanged: (val) {
                    setState(() {
                      _searchQuery = val;
                      _applyFilter();
                    });
                  },
                  decoration: InputDecoration(
                    hintText: "Search Sovereign Notes...",
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                    prefixIcon: Icon(Icons.search, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                    suffixIcon: _searchQuery.isNotEmpty
                        ? IconButton(
                            onPressed: () {
                              _searchController.clear();
                              setState(() {
                                _searchQuery = '';
                                _applyFilter();
                              });
                            },
                            icon: Icon(Icons.clear, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                          )
                        : null,
                    filled: true,
                    fillColor: AppTheme.current.text.withValues(alpha: 0.04),
                    contentPadding: EdgeInsets.zero,
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(12),
                      borderSide: BorderSide.none,
                    ),
                  ),
                ),
              ],
            ),
          ),
          // Search results indicator
          if (_searchQuery.isNotEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              child: Row(
                children: [
                  Icon(Icons.search, size: 14, color: AppTheme.current.accent),
                  SizedBox(width: 6),
                  Text(
                    '${_filteredNotes.length} result${_filteredNotes.length == 1 ? '' : 's'}',
                    style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.w600),
                  ),
                ],
              ),
            ),
          // Notes list
          Expanded(
            child: _isLoading
                ? Center(child: CircularProgressIndicator(color: AppTheme.current.accent))
                : _filteredNotes.isEmpty
                    ? _buildEmptyState()
                    : _buildNotesList(),
          ),
        ],
      ),
      floatingActionButton: Padding(
        padding: const EdgeInsets.only(bottom: 100),
        child: FloatingActionButton(
          heroTag: null,
          onPressed: _createNote,
          backgroundColor: AppTheme.current.accent,
          child: Icon(Icons.add, color: Colors.black),
        ),
      ),
      floatingActionButtonLocation: FloatingActionButtonLocation.endFloat,
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.sticky_note_2_outlined, size: 64, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
          SizedBox(height: 16),
          Text(
            _searchQuery.isNotEmpty ? "No notes found" : "Sovereign Notes is Empty",
            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.7), fontSize: 16, fontWeight: FontWeight.bold),
          ),
          SizedBox(height: 8),
          Text(
            _searchQuery.isNotEmpty ? "Try a different search term" : "Tap + to create your first note.",
            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 12),
          ),
        ],
      ),
    );
  }

  Widget _buildNotesList() {
    return ListView.builder(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      itemCount: _filteredNotes.length,
      itemBuilder: (context, index) {
        final note = _filteredNotes[index];
        return _buildNoteCard(note);
      },
    );
  }

  Widget _buildNoteCard(dynamic note) {
    final title = note['title'] as String? ?? 'Untitled';
    final content = note['content'] as String? ?? '';
    final tags = _parseTags(note['tags']);
    final updatedAt = note['updated_at'] as String?;
    final imagePath = note['image_path'] as String?;
    final hasImage = imagePath != null && imagePath.isNotEmpty && File(imagePath).existsSync();

    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: () => _openNote(note['id']),
        borderRadius: BorderRadius.circular(12),
        splashColor: AppTheme.current.accent.withValues(alpha: 0.08),
        child: GlassmorphicContainer(
          margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          padding: const EdgeInsets.all(14),
          borderRadius: BorderRadius.circular(12),
          tintColor: AppTheme.current.accent,
          blur: 10,
          tintAlpha: 0.08,
          borderAlpha: 0.12,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Note icon
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  color: (hasImage ? Colors.purpleAccent : AppTheme.current.accent).withValues(alpha: 0.08),
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: (hasImage ? Colors.purpleAccent : AppTheme.current.accent).withValues(alpha: 0.2), width: 1),
                ),
                child: Icon(
                  hasImage ? Icons.image_rounded : Icons.sticky_note_2_rounded,
                  color: hasImage ? Colors.purpleAccent : AppTheme.current.accent,
                  size: 22,
                ),
              ),
              const SizedBox(width: 12),
              // Note content (middle)
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            title,
                            style: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.bold),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        Text(
                          _formatDate(updatedAt),
                          style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 11),
                        ),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      _getPreview(content),
                      style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.6), fontSize: 12, height: 1.4),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    if (tags.isNotEmpty) ...[
                      const SizedBox(height: 6),
                      Wrap(
                        spacing: 4,
                        runSpacing: 4,
                        children: tags.take(3).map((tag) => Container(
                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                          decoration: BoxDecoration(
                            color: AppTheme.current.accent.withValues(alpha: 0.1),
                            borderRadius: BorderRadius.circular(6),
                            border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                          ),
                          child: Text(tag, style: TextStyle(color: AppTheme.current.accent, fontSize: 9, fontWeight: FontWeight.w500)),
                        )).toList(),
                      ),
                    ],
                  ],
                ),
              ),
              // Image thumbnail (right side)
              if (hasImage) ...[
                const SizedBox(width: 10),
                ClipRRect(
                  borderRadius: BorderRadius.circular(8),
                  child: Image.file(
                    File(imagePath),
                    width: 48,
                    height: 48,
                    fit: BoxFit.cover,
                    errorBuilder: (context, error, stackTrace) => Container(
                      width: 48,
                      height: 48,
                      decoration: BoxDecoration(
                        color: AppTheme.current.mutedText.withValues(alpha: 0.1),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Icon(Icons.broken_image, color: AppTheme.current.mutedText.withValues(alpha: 0.3), size: 20),
                    ),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  void _createNote() {
    Navigator.push(
      context,
      MaterialPageRoute(builder: (context) => NoteEditorScreen(
        isNew: true,
        onSave: (title, content, tags, imagePath) {
          final id = 'note_${const Uuid().v4()}';
          _client.noteCreate(id, title, content, json.encode(tags), imagePath);
          _loadNotes();
        },
      )),
    );
  }

  void _openNote(String id) {
    final note = _client.noteGet(id);
    if (note == null) return;

    Navigator.push(
      context,
      MaterialPageRoute(builder: (context) => NoteDetailScreen(
        note: note,
        onUpdate: (title, content, tags, imagePath) {
          _client.noteSaveVersion(id, note['title'] ?? '', note['content'] ?? '', note['tags'] ?? '[]');
          _client.noteUpdate(id, title, content, json.encode(tags), imagePath);
          _loadNotes();
        },
        onDelete: () {
          _client.noteDelete(id);
          _loadNotes();
          Navigator.pop(context);
        },
        onShare: (noteData) => _shareNoteToChat(noteData),
      )),
    );
  }

  void _shareNoteToChat(Map<String, dynamic> note) {
    final title = note['title'] as String? ?? 'Note';
    final content = note['content'] as String? ?? '';

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) {
        final contacts = _client.getContacts();
        final groups = _client.getAllGroups();
        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: Text(
                  "Share \"$title\"",
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
                Flexible(
                  child: ListView(
                    children: [
                      if (contacts.isNotEmpty) ...[
                        Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Text("DIRECT CHATS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold)),
                        ),
                        ...contacts.map((c) => Material(color: Colors.transparent, child: ListTile(
                          leading: Icon(Icons.person, color: AppTheme.current.mutedText),
                          title: Text(c['alias']?.toString().isNotEmpty == true ? c['alias'] : (c['global_name'] ?? c['peer_id']), style: TextStyle(color: AppTheme.current.text)),
                          onTap: () {
                            final msg = "[NOTE]:$title\n$content";
                            _client.sendMessage(c['peer_id'], msg);
                            Navigator.pop(ctx);
                            ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Note shared.")));
                          },
                        ))),
                      ],
                      if (groups.isNotEmpty) ...[
                        Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Text("MESH ROOMS", style: TextStyle(color: AppTheme.current.accent, fontSize: 10, letterSpacing: 1.2, fontWeight: FontWeight.bold)),
                        ),
                        ...groups.map((g) => Material(color: Colors.transparent, child: ListTile(
                          leading: Icon(Icons.group, color: AppTheme.current.mutedText),
                          title: Text(g[1].toString(), style: TextStyle(color: AppTheme.current.text)),
                          onTap: () {
                            final msg = "[NOTE]:$title\n$content";
                            _client.sendGroupMessage(g[0], msg);
                            Navigator.pop(ctx);
                            ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Note shared to group.")));
                          },
                        ))),
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

  void _showImportDialog() {
    final passwordController = TextEditingController();

    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("IMPORT NOTES", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text("Import notes from a Sovereign Notes archive.", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)),
            const SizedBox(height: 16),
            TextField(
              controller: passwordController,
              obscureText: true,
              style: TextStyle(color: AppTheme.current.text),
              decoration: InputDecoration(
                labelText: 'Password (if encrypted)',
                labelStyle: TextStyle(color: AppTheme.current.mutedText),
                filled: true,
                fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText))),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextButton(
                onPressed: () { Navigator.pop(ctx); _importNotes(passwordController.text, replace: false); },
                child: Text("MERGE", style: TextStyle(color: AppTheme.current.accent)),
              ),
              TextButton(
                onPressed: () { Navigator.pop(ctx); _showImportConfirmReplace(passwordController.text); },
                child: Text("REPLACE", style: TextStyle(color: Colors.redAccent)),
              ),
            ],
          ),
        ],
      ),
    ).whenComplete(() => passwordController.dispose());
  }

  void _showImportConfirmReplace(String password) {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: const Text("Replace All Notes?", style: TextStyle(color: Colors.redAccent)),
        content: Text("This will DELETE all current notes and replace them with the imported notes.", style: TextStyle(color: AppTheme.current.text)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText))),
          TextButton(
            onPressed: () { Navigator.pop(ctx); _importNotes(password, replace: true); },
            child: const Text("REPLACE ALL", style: TextStyle(color: Colors.redAccent)),
          ),
        ],
      ),
    );
  }

  Future<void> _importNotes(String password, {required bool replace}) async {
    try {
      final result = await FilePicker.platform.pickFiles(type: FileType.custom, allowedExtensions: ['json']);
      if (result == null || result.files.isEmpty) return;

      final file = File(result.files.single.path!);
      var jsonContent = await file.readAsString();

      if (password.isNotEmpty) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Password protection removed in this version. Importing as plain text.")));
      }

      final data = json.decode(jsonContent) as Map<String, dynamic>;
      final importedNotes = data['notes'] as List<dynamic>? ?? [];
      if (importedNotes.isEmpty) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("No notes found in archive")));
        return;
      }

      if (replace) {
        final currentNotes = _client.notesGetAll();
        for (var n in currentNotes) { _client.noteDelete(n['id']); }
      }

      int imported = 0;
      for (var note in importedNotes) {
        try {
          final id = note['id'] as String? ?? 'note_${const Uuid().v4()}';
          final title = note['title'] as String? ?? 'Untitled';
          final content = note['content'] as String? ?? '';
          final tags = note['tags'] as String? ?? '[]';
          final imagePath = note['image_path'] as String?;
          final existing = _client.noteGet(id);
          if (existing != null && !replace) {
            _client.noteUpdate(id, title, content, tags, imagePath);
          } else {
            _client.noteCreate(id, title, content, tags, imagePath);
          }
          imported++;
        } catch (_) {}
      }
      _loadNotes();
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Imported $imported notes${replace ? ' (replaced all)' : ' (merged)'}")));
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Import failed: $e")));
    }
  }

  void _showExportDialog() {
    final filenameController = TextEditingController(text: 'sovereign_notes');
    final passwordController = TextEditingController();

    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        title: Text("EXPORT NOTES", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text("Export all notes as a JSON archive.", style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 13)),
            const SizedBox(height: 16),
            TextField(
              controller: filenameController,
              style: TextStyle(color: AppTheme.current.text),
              decoration: InputDecoration(
                labelText: 'File name', labelStyle: TextStyle(color: AppTheme.current.mutedText),
                filled: true, fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: passwordController, obscureText: true,
              style: TextStyle(color: AppTheme.current.text),
              decoration: InputDecoration(
                labelText: 'Password (optional)', labelStyle: TextStyle(color: AppTheme.current.mutedText),
                filled: true, fillColor: AppTheme.current.mutedText.withValues(alpha: 0.1),
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText))),
          ElevatedButton(
            onPressed: () { Navigator.pop(ctx); _exportNotes(filenameController.text, passwordController.text); },
            style: ElevatedButton.styleFrom(backgroundColor: AppTheme.current.accent, foregroundColor: Colors.black),
            child: const Text("EXPORT"),
          ),
        ],
      ),
    ).whenComplete(() {
      filenameController.dispose();
      passwordController.dispose();
    });
  }

  Future<void> _exportNotes(String filename, String password) async {
    try {
      final notesData = {
        'version': '1.0',
        'exported_at': DateTime.now().toIso8601String(),
        'password_protected': false, // XOR removed — was not real encryption
        'notes': _notes.map((n) => {
          'id': n['id'], 'title': n['title'], 'content': n['content'],
          'tags': n['tags'], 'image_path': n['image_path'],
          'created_at': n['created_at'], 'updated_at': n['updated_at'],
        }).toList(),
      };
      String jsonContent = json.encode(notesData);
      final dir = await getApplicationDocumentsDirectory();
      final file = File('${dir.path}/$filename.json');
      await file.writeAsString(jsonContent);
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Notes exported to ${file.path}")));
    } catch (e) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Export failed: $e")));
    }
  }

  void _showHelpDialog() {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: AppTheme.current.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        title: Row(children: [
          Icon(Icons.help_outline, color: AppTheme.current.accent),
          const SizedBox(width: 8),
          Text("SOVEREIGN NOTES", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1)),
        ]),
        content: SingleChildScrollView(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            _helpSection("CREATE", "Tap + to create a note. Add title, content, tags (comma-separated), and optionally attach an image."),
            _helpSection("IMAGES", "In the note editor, tap the image icon to pick a photo from your gallery or camera."),
            _helpSection("EDIT", "Open a note, tap Edit in the top right. Save when done."),
            _helpSection("SEARCH", "Search bar filters by title, content, or tags. Partial matches work."),
            _helpSection("TAGS", "Add comma-separated tags when creating or editing notes."),
            _helpSection("VERSION HISTORY", "Open a note, tap the clock icon to see and restore previous versions."),
            _helpSection("SHARE", "Open a note, tap the share icon to send it to a contact or group."),
            _helpSection("EXPORT", "Menu (⋮) → Export Notes. JSON archive with optional password encryption."),
            _helpSection("IMPORT", "Menu (⋮) → Import Notes. Choose Merge (add) or Replace (delete all first)."),
            _helpSection("DELETE", "Open a note, tap the menu (⋮) → Delete."),
          ]),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text("GOT IT", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold))),
        ],
      ),
    );
  }

  Widget _helpSection(String title, String description) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(title, style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 0.5)),
        const SizedBox(height: 2),
        Text(description, style: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.7), fontSize: 12, height: 1.4)),
      ]),
    );
  }
}

// ==================== NOTE DETAIL SCREEN ====================

class NoteDetailScreen extends StatefulWidget {
  final Map<String, dynamic> note;
  final Function(String title, String content, List<String> tags, String? imagePath) onUpdate;
  final VoidCallback onDelete;
  final Function(Map<String, dynamic>) onShare;

  const NoteDetailScreen({required this.note, required this.onUpdate, required this.onDelete, required this.onShare, super.key});

  @override
  State<NoteDetailScreen> createState() => _NoteDetailScreenState();
}

class _NoteDetailScreenState extends State<NoteDetailScreen> {
  bool _isEditing = false;
  late TextEditingController _titleController;
  late TextEditingController _contentController;
  late TextEditingController _tagsController;
  String? _imagePath;

  @override
  void initState() {
    super.initState();
    _titleController = TextEditingController(text: widget.note['title'] ?? '');
    _contentController = TextEditingController(text: widget.note['content'] ?? '');
    final tags = widget.note['tags'] as String? ?? '[]';
    try { _tagsController = TextEditingController(text: List<String>.from(json.decode(tags)).join(', ')); } catch (_) { _tagsController = TextEditingController(); }
    _imagePath = widget.note['image_path'];
  }

  @override
  void dispose() { _titleController.dispose(); _contentController.dispose(); _tagsController.dispose(); super.dispose(); }

  void _saveNote() {
    final tags = _tagsController.text.split(',').map((t) => t.trim()).where((t) => t.isNotEmpty).toList();
    widget.onUpdate(_titleController.text, _contentController.text, tags, _imagePath);
    setState(() => _isEditing = false);
    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Note saved")));
  }

  Future<void> _pickImage() async {
    try {
      final picked = await ImagePicker().pickImage(source: ImageSource.gallery, imageQuality: 80);
      if (picked != null) {
        final dir = await getApplicationDocumentsDirectory();
        final fileName = 'note_img_${DateTime.now().millisecondsSinceEpoch}.jpg';
        final savedFile = await File(picked.path).copy('${dir.path}/$fileName');
        if (!mounted) return;
        setState(() => _imagePath = savedFile.path);
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to pick image: $e")));
      }
    }
  }

  void _showVersionHistory() {
    final versions = IntrovertClient().noteGetVersions(widget.note['id']);
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: AppTheme.current.surface,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(20))),
      builder: (ctx) => DraggableScrollableSheet(
        initialChildSize: 0.6, minChildSize: 0.3, maxChildSize: 0.9, expand: false,
        builder: (_, scrollController) => Column(children: [
          const SizedBox(height: 12),
          Container(width: 40, height: 4, decoration: BoxDecoration(color: AppTheme.current.mutedText.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(2))),
          Padding(padding: const EdgeInsets.all(16), child: Text("VERSION HISTORY", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold, letterSpacing: 1))),
          Flexible(child: versions.isEmpty
              ? Center(child: Text("No version history", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5))))
              : ListView.builder(controller: scrollController, itemCount: versions.length, itemBuilder: (context, index) {
                  final v = versions[index];
                  return ListTile(
                    leading: CircleAvatar(backgroundColor: AppTheme.current.accent.withValues(alpha: 0.2), child: Text("v${v['version']}", style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.bold))),
                    title: Text(v['title'] ?? '', style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                    subtitle: Text(_formatDate(v['created_at']), style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 11)),
                    trailing: IconButton(icon: Icon(Icons.restore, color: AppTheme.current.accent, size: 20), onPressed: () {
                      Navigator.pop(ctx);
                      setState(() {
                        _titleController.text = v['title'] ?? '';
                        _contentController.text = v['content'] ?? '';
                        try { _tagsController.text = List<String>.from(json.decode(v['tags'] ?? '[]')).join(', '); } catch (_) { _tagsController.text = ''; }
                        _isEditing = true;
                      });
                    }),
                  );
                })),
          const SizedBox(height: 16),
        ]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final tags = _parseTags(widget.note['tags']);
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        backgroundColor: AppTheme.current.surface,
        leading: IconButton(icon: const Icon(Icons.arrow_back), onPressed: () => Navigator.pop(context)),
        title: Text(_isEditing ? 'EDIT NOTE' : 'NOTE', style: TextStyle(fontSize: 14, fontWeight: FontWeight.bold, letterSpacing: 1)),
        actions: [
          if (!_isEditing) ...[
            IconButton(onPressed: () => widget.onShare(widget.note), icon: Icon(Icons.share_outlined, color: AppTheme.current.mutedText), tooltip: 'Share'),
            IconButton(onPressed: _showVersionHistory, icon: Icon(Icons.history, color: AppTheme.current.mutedText), tooltip: 'History'),
            PopupMenuButton(icon: Icon(Icons.more_vert, color: AppTheme.current.mutedText), onSelected: (val) {
              if (val == 'edit') setState(() => _isEditing = true);
              if (val == 'delete') _confirmDelete();
            }, itemBuilder: (_) => [const PopupMenuItem(value: 'edit', child: Text('Edit')), const PopupMenuItem(value: 'delete', child: Text('Delete', style: TextStyle(color: Colors.redAccent)))]),
          ] else ...[
            TextButton(onPressed: () => setState(() => _isEditing = false), child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText))),
            TextButton(onPressed: _saveNote, child: Text("SAVE", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold))),
          ],
        ],
      ),
      body: _isEditing ? _buildEditMode() : _buildViewMode(tags),
    );
  }

  Widget _buildViewMode(List<String> tags) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(20),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(widget.note['title'] ?? '', style: TextStyle(color: AppTheme.current.text, fontSize: 24, fontWeight: FontWeight.bold)),
        const SizedBox(height: 8),
        Text('Edited ${_formatDate(widget.note['updated_at'])}', style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 12)),
        if (tags.isNotEmpty) ...[
          const SizedBox(height: 12),
          Wrap(spacing: 6, runSpacing: 4, children: tags.map((tag) => Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
            decoration: BoxDecoration(color: AppTheme.current.accent.withValues(alpha: 0.1), borderRadius: BorderRadius.circular(10), border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2))),
            child: Text(tag, style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.w500)),
          )).toList()),
        ],
        if (_imagePath != null && _imagePath!.isNotEmpty && File(_imagePath!).existsSync()) ...[
          const SizedBox(height: 16),
          ClipRRect(borderRadius: BorderRadius.circular(12), child: Image.file(File(_imagePath!), fit: BoxFit.cover, width: double.infinity)),
        ],
        const SizedBox(height: 20),
        Text(widget.note['content'] ?? '', style: TextStyle(color: AppTheme.current.text, fontSize: 15, height: 1.6)),
      ]),
    );
  }

  Widget _buildEditMode() {
    return Column(
      children: [
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(20, 16, 20, 80),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              // Image preview if set
              if (_imagePath != null && _imagePath!.isNotEmpty && File(_imagePath!).existsSync()) ...[
                Stack(children: [
                  ClipRRect(borderRadius: BorderRadius.circular(12), child: Image.file(File(_imagePath!), fit: BoxFit.cover, width: double.infinity, height: 180)),
                  Positioned(top: 8, right: 8, child: GestureDetector(
                    onTap: () => setState(() => _imagePath = null),
                    child: Container(padding: const EdgeInsets.all(4), decoration: BoxDecoration(color: Colors.black54, shape: BoxShape.circle), child: Icon(Icons.close, color: Colors.white, size: 16)),
                  )),
                ]),
                const SizedBox(height: 12),
              ],
              // Title
              TextField(controller: _titleController, style: TextStyle(color: AppTheme.current.text, fontSize: 24, fontWeight: FontWeight.bold),
                decoration: InputDecoration(hintText: 'Note title', hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3)), border: InputBorder.none)),
              const SizedBox(height: 4),
              // Content
              TextField(controller: _contentController, style: TextStyle(color: AppTheme.current.text, fontSize: 15, height: 1.6), maxLines: null, keyboardType: TextInputType.multiline,
                decoration: InputDecoration(hintText: 'Write your note...', hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3)), border: InputBorder.none)),
            ]),
          ),
        ),
        // Bottom bar: tags + image button
        Container(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
          decoration: BoxDecoration(
            color: AppTheme.current.surface,
            border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
          ),
          child: SafeArea(
            top: false,
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _tagsController,
                    style: TextStyle(color: AppTheme.current.text, fontSize: 12),
                    decoration: InputDecoration(
                      hintText: 'Tags (comma separated)',
                      hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.4), fontSize: 12),
                      isDense: true,
                      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                      filled: true,
                      fillColor: AppTheme.current.mutedText.withValues(alpha: 0.05),
                      border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
                      prefixIcon: Icon(Icons.tag, color: AppTheme.current.mutedText.withValues(alpha: 0.4), size: 16),
                      prefixIconConstraints: BoxConstraints(minWidth: 32),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                GestureDetector(
                  onTap: _pickImage,
                  child: Container(
                    width: 40,
                    height: 40,
                    decoration: BoxDecoration(
                      color: _imagePath != null ? AppTheme.current.accent.withValues(alpha: 0.15) : AppTheme.current.mutedText.withValues(alpha: 0.08),
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(color: _imagePath != null ? AppTheme.current.accent.withValues(alpha: 0.4) : AppTheme.current.mutedText.withValues(alpha: 0.15), width: 1),
                    ),
                    child: Icon(
                      _imagePath != null ? Icons.image_rounded : Icons.add_photo_alternate_outlined,
                      color: _imagePath != null ? AppTheme.current.accent : AppTheme.current.mutedText.withValues(alpha: 0.5),
                      size: 20,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  void _confirmDelete() {
    showDialog(context: context, builder: (ctx) => AlertDialog(
      backgroundColor: AppTheme.current.surface,
      title: const Text("Delete Note?", style: TextStyle(color: Colors.redAccent)),
      content: Text("This action cannot be undone.", style: TextStyle(color: AppTheme.current.text)),
      actions: [
        TextButton(onPressed: () => Navigator.pop(ctx), child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText))),
        TextButton(onPressed: () { Navigator.pop(ctx); widget.onDelete(); }, child: const Text("DELETE", style: TextStyle(color: Colors.redAccent))),
      ],
    ));
  }

  String _formatDate(String? dateStr) {
    if (dateStr == null || dateStr.isEmpty) return '';
    try { final date = DateTime.parse(dateStr.replaceFirst(' ', 'T')).toLocal(); return '${date.day}/${date.month}/${date.year} ${date.hour}:${date.minute.toString().padLeft(2, '0')}'; } catch (_) { return dateStr; }
  }

  List<String> _parseTags(String? tagsStr) {
    if (tagsStr == null || tagsStr.isEmpty) return [];
    try { return List<String>.from(json.decode(tagsStr)); } catch (_) { return []; }
  }
}

// ==================== NOTE EDITOR SCREEN ====================

class NoteEditorScreen extends StatefulWidget {
  final bool isNew;
  final Function(String title, String content, List<String> tags, String? imagePath) onSave;
  const NoteEditorScreen({required this.isNew, required this.onSave, super.key});
  @override
  State<NoteEditorScreen> createState() => _NoteEditorScreenState();
}

class _NoteEditorScreenState extends State<NoteEditorScreen> {
  final TextEditingController _titleController = TextEditingController();
  final TextEditingController _contentController = TextEditingController();
  final TextEditingController _tagsController = TextEditingController();
  String? _imagePath;

  @override
  void dispose() { _titleController.dispose(); _contentController.dispose(); _tagsController.dispose(); super.dispose(); }

  void _save() {
    final title = _titleController.text.trim();
    if (title.isEmpty) { ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text("Title is required"))); return; }
    final tags = _tagsController.text.split(',').map((t) => t.trim()).where((t) => t.isNotEmpty).toList();
    widget.onSave(title, _contentController.text, tags, _imagePath);
    Navigator.pop(context);
  }

  Future<void> _pickImage() async {
    try {
      final picked = await ImagePicker().pickImage(source: ImageSource.gallery, imageQuality: 80);
      if (picked != null) {
        final dir = await getApplicationDocumentsDirectory();
        final fileName = 'note_img_${DateTime.now().millisecondsSinceEpoch}.jpg';
        final savedFile = await File(picked.path).copy('${dir.path}/$fileName');
        if (!mounted) return;
        setState(() => _imagePath = savedFile.path);
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Failed to pick image: $e")));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        backgroundColor: AppTheme.current.surface,
        leading: IconButton(icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context)),
        title: const Text('NEW NOTE', style: TextStyle(fontSize: 14, fontWeight: FontWeight.bold, letterSpacing: 1)),
        actions: [TextButton(onPressed: _save, child: Text("SAVE", style: TextStyle(color: AppTheme.current.accent, fontWeight: FontWeight.bold)))],
      ),
      body: Column(
        children: [
          Expanded(
            child: SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(20, 16, 20, 80),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                // Note title
                TextField(
                  controller: _titleController,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 24, fontWeight: FontWeight.bold),
                  decoration: InputDecoration(
                    hintText: 'Note title',
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3)),
                    border: InputBorder.none,
                  ),
                ),
                const SizedBox(height: 4),
                // Note content (main text area)
                TextField(
                  controller: _contentController,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 15, height: 1.6),
                  maxLines: null,
                  keyboardType: TextInputType.multiline,
                  autofocus: true,
                  decoration: InputDecoration(
                    hintText: 'Write your note...',
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3)),
                    border: InputBorder.none,
                  ),
                ),
              ]),
            ),
          ),
          // Bottom bar: tags + image button
          Container(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
            decoration: BoxDecoration(
              color: AppTheme.current.surface,
              border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.1))),
            ),
            child: SafeArea(
              top: false,
              child: Row(
                children: [
                  // Tags field
                  Expanded(
                    child: TextField(
                      controller: _tagsController,
                      style: TextStyle(color: AppTheme.current.text, fontSize: 12),
                      decoration: InputDecoration(
                        hintText: 'Tags (comma separated)',
                        hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.4), fontSize: 12),
                        isDense: true,
                        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                        filled: true,
                        fillColor: AppTheme.current.mutedText.withValues(alpha: 0.05),
                        border: OutlineInputBorder(
                          borderRadius: BorderRadius.circular(8),
                          borderSide: BorderSide.none,
                        ),
                        prefixIcon: Icon(Icons.tag, color: AppTheme.current.mutedText.withValues(alpha: 0.4), size: 16),
                        prefixIconConstraints: BoxConstraints(minWidth: 32),
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  // Image attach button
                  GestureDetector(
                    onTap: _pickImage,
                    child: Container(
                      width: 40,
                      height: 40,
                      decoration: BoxDecoration(
                        color: _imagePath != null
                            ? AppTheme.current.accent.withValues(alpha: 0.15)
                            : AppTheme.current.mutedText.withValues(alpha: 0.08),
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(
                          color: _imagePath != null
                              ? AppTheme.current.accent.withValues(alpha: 0.4)
                              : AppTheme.current.mutedText.withValues(alpha: 0.15),
                          width: 1,
                        ),
                      ),
                      child: Icon(
                        _imagePath != null ? Icons.image_rounded : Icons.add_photo_alternate_outlined,
                        color: _imagePath != null ? AppTheme.current.accent : AppTheme.current.mutedText.withValues(alpha: 0.5),
                        size: 20,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
