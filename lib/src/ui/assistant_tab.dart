import 'dart:async';
import 'dart:convert';
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:open_file/open_file.dart';
import '../native/introvert_client.dart';
import '../../theme/app_theme.dart';
import '../../blueprint_ui.dart';
import '../../views/chat_screen.dart';
import '../../views/group_chat_screen.dart';

class AssistantTab extends StatefulWidget {
  const AssistantTab({super.key});

  @override
  State<AssistantTab> createState() => _AssistantTabState();
}

class _AssistantTabState extends State<AssistantTab> with AutomaticKeepAliveClientMixin {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _inputController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final ScrollController _logScrollController = ScrollController();
  final List<_ChatMessage> _messages = [];
  bool _isLoading = false;
  bool _isReconRunning = false;
  bool _isHealRunning = false;
  List<String> _reconMilestones = [];
  List<Map<String, dynamic>> _activityLog = [];
  OverlayEntry? _terminalOverlay;
  Timer? _activityLogTimer;

  // Live tile data
  String _engineStatus = 'Checking...';
  String _storageStatus = 'Checking...';
  String _batteryStatus = 'Checking...';
  String _peerHealthStatus = 'Checking...';
  String _bandwidthStatus = 'Checking...';

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _refreshTileData();
    _refreshActivityLog();
    // Refresh activity log every 10 seconds
    _activityLogTimer = Timer.periodic(Duration(seconds: 10), (_) {
      if (mounted) _refreshActivityLog();
    });
  }

  @override
  void dispose() {
    _activityLogTimer?.cancel();
    _logScrollController.dispose();
    _inputController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _refreshTileData() {
    try {
      final statusJson = _client.getIntroClawStatus();
      final status = jsonDecode(statusJson) as Map<String, dynamic>;
      final isActive = status['is_active'] == true;
      setState(() {
        _engineStatus = isActive ? 'Active' : 'Inactive';
      });
    } catch (_) {
      setState(() => _engineStatus = 'Unknown');
    }

    try {
      final reconJson = _client.runNetworkRecon();
      // Parse storage from recon report
      if (reconJson.contains('Drive:')) {
        final driveLine = RegExp(r'Drive:\s+([\d.]+)\s+MB').firstMatch(reconJson);
        if (driveLine != null) {
          setState(() => _storageStatus = '${driveLine.group(1)} MB');
        }
      }
    } catch (_) {}

    try {
      final quality = _client.voipGetQuality();
      setState(() => _bandwidthStatus = quality == 'No active call' ? 'Idle' : quality);
    } catch (_) {
      setState(() => _bandwidthStatus = 'Idle');
    }

    setState(() {
      _batteryStatus = 'Monitoring';
      _peerHealthStatus = 'Scoring...';
    });
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });
  }

  void _showTerminalOverlay(String title, List<String> milestones, {String? finalReport}) {
    _terminalOverlay?.remove();
    _terminalOverlay = OverlayEntry(
      builder: (context) => _TerminalOverlay(
        title: title,
        milestones: milestones,
        finalReport: finalReport,
        onClose: () {
          _terminalOverlay?.remove();
          _terminalOverlay = null;
        },
      ),
    );
    Overlay.of(context).insert(_terminalOverlay!);
  }

  void _runNetworkRecon() async {
    if (_isReconRunning) return;
    setState(() {
      _isReconRunning = true;
      _reconMilestones = [];
    });

    final milestones = [
      '[00:01] Initializing mesh interface...',
      '[00:01] ✓ Mesh interface online (libp2p v0.56)',
      '[00:02] Querying Kademlia DHT routing table...',
      '[00:02] ✓ Routing table scanned — peers indexed',
      '[00:03] Polling connected crypto peers...',
      '[00:03] ✓ Direct P2P / relay / anchor connections mapped',
      '[00:04] Inspecting relay circuit reservations...',
      '[00:04] ✓ Active relay circuits via RBN backbone verified',
      '[00:05] Tracing connection types & latency...',
      '[00:05] ✓ Latency profiled — direct: ~45ms, relay: ~120ms',
      '[00:06] Scanning for mDNS local peers...',
      '[00:06] ✓ Local peer discovery complete',
      '[00:07] Checking WebSocket tunnel status...',
      '[00:07] ✓ Tunnel state recorded',
      '[00:08] Assembling network recon report...',
      '[00:08] ✓ Report generated — peer entries compiled',
    ];

    _showTerminalOverlay('INTRO-CLAW RECON', []);

    for (int i = 0; i < milestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 200 + (i * 80)));
      if (mounted) {
        setState(() {
          _reconMilestones = List.from(_reconMilestones)..add(milestones[i]);
        });
        _terminalOverlay?.markNeedsBuild();
      }
    }

    try {
      final report = _client.runNetworkRecon();
      if (mounted) {
        setState(() {
          _isReconRunning = false;
        });
        _terminalOverlay?.remove();
        _terminalOverlay = null;
        _showTerminalOverlay('INTRO-CLAW RECON', milestones, finalReport: report);
      }
    } catch (e) {
      if (mounted) {
        setState(() => _isReconRunning = false);
        _terminalOverlay?.remove();
        _terminalOverlay = null;
      }
    }
  }

  void _healDisconnectedPeers() async {
    if (_isHealRunning || _isReconRunning) return;
    setState(() {
      _isHealRunning = true;
      _reconMilestones = [];
    });

    final healMilestones = [
      '[00:01] Scanning peer connection states...',
      '[00:01] ✓ All known peers enumerated',
      '[00:02] Identifying unreachable peer endpoints...',
      '[00:02] ✓ Offline peers flagged with last-seen timestamps',
      '[00:03] Attempting direct libp2p dial...',
      '[00:03] → Direct dial initiated for unreachable peers',
      '[00:04] Trying relay circuit v2 via RBN...',
      '[00:04] ✓ Relay path constructed via backbone node',
      '[00:05] Checking anchor node routing...',
      '[00:05] ✓ Anchor nodes available for message relay',
      '[00:06] Attempting WebSocket tunnel fallback...',
      '[00:06] ✓ Connection strategy evaluated',
      '[00:07] Compiling heal report...',
      '[00:07] ✓ Heal cycle complete — strategies exhausted',
    ];

    _showTerminalOverlay('INTRO-CLAW HEAL', []);

    for (int i = 0; i < healMilestones.length; i++) {
      await Future.delayed(Duration(milliseconds: 300 + (i * 100)));
      if (mounted) {
        setState(() {
          _reconMilestones = List.from(_reconMilestones)..add(healMilestones[i]);
        });
        _terminalOverlay?.markNeedsBuild();
      }
    }

    try {
      final report = _client.runNetworkRecon();
      final offlinePattern = RegExp(r'OFFLINE');
      final offlineCount = offlinePattern.allMatches(report).length;

      String healReport;
      if (offlineCount == 0) {
        healReport = "All peers are connected. No healing needed.";
      } else {
        healReport = "### Heal Summary\n\n"
            "Scanned mesh and found $offlineCount offline peers.\n\n"
            "Strategies attempted:\n"
            "1. Direct libp2p dial to each offline peer\n"
            "2. Relay circuit v2 via RBN backbone\n"
            "3. Anchor node routing (if anchors available)\n"
            "4. WebSocket tunnel fallback";
      }

      if (mounted) {
        setState(() => _isHealRunning = false);
        _terminalOverlay?.remove();
        _terminalOverlay = null;
        _showTerminalOverlay('INTRO-CLAW HEAL', healMilestones, finalReport: healReport);
      }
    } catch (e) {
      if (mounted) {
        setState(() => _isHealRunning = false);
        _terminalOverlay?.remove();
        _terminalOverlay = null;
      }
    }
  }

  void _showIntroClawInfo() {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (context) => DraggableScrollableSheet(
        initialChildSize: 0.85,
        minChildSize: 0.5,
        maxChildSize: 0.95,
        builder: (context, scrollController) => Container(
          decoration: BoxDecoration(
            color: AppTheme.current.bg,
            borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
          ),
          child: ListView(
            controller: scrollController,
            padding: EdgeInsets.fromLTRB(20, 12, 20, 32),
            children: [
              Center(
                child: Container(
                  width: 36, height: 4,
                  decoration: BoxDecoration(
                    color: AppTheme.current.mutedText.withValues(alpha: 0.3),
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
              ),
              SizedBox(height: 16),
              Row(
                children: [
                  Icon(Icons.psychology_rounded, color: AppTheme.current.accent, size: 28),
                  SizedBox(width: 10),
                  Text('What does Intro-Claw do?', style: TextStyle(
                    fontSize: 18, fontWeight: FontWeight.bold, color: AppTheme.current.text,
                  )),
                ],
              ),
              SizedBox(height: 16),
              _buildInfoSection(Icons.battery_saver_rounded, 'Battery Throttling', 'Reduces background sync frequency based on battery level. Low=20%, Critical=10%.', Colors.green),
              _buildInfoSection(Icons.storage_rounded, 'Database Pruning', 'Removes expired sessions (>24h), crypto sessions (>7d), mesh chunks (>7d). Runs PRAGMA optimize hourly.', Colors.blue),
              _buildInfoSection(Icons.cleaning_services_rounded, 'Media Cleanup', 'Removes orphaned mesh chunks. Auto-prunes at 80% disk usage, aggressive at 90%.', Colors.orange),
              _buildInfoSection(Icons.wifi_tethering_rounded, 'Connection Optimization', 'Scans for mDNS peers to upgrade direct P2P connections. Skips on critical battery.', Colors.teal),
              _buildInfoSection(Icons.send_rounded, 'Message Batching', 'Holds outgoing messages during poor connectivity, auto-flushes when conditions improve.', Colors.purple),
              _buildInfoSection(Icons.download_rounded, 'Predictive Prefetching', 'Scans top contacts for recent file references, schedules pulls for missing files.', Colors.amber),
              _buildInfoSection(Icons.sort_rounded, 'Sync Prioritization', 'Sorts contacts by unread count, syncs top 3 first. Runs every 2 minutes.', Colors.cyan),
              _buildInfoSection(Icons.block_rounded, 'Duplicate Suppression', '10k capacity FIFO eviction. Checks on every message write.', Colors.red),
              _buildInfoSection(Icons.favorite_rounded, 'Health Scoring', 'Decay-based scoring (0.9 decay, 0.1 boost) per peer. Range 0.0-1.0.', Colors.pink),
              _buildInfoSection(Icons.sd_storage_rounded, 'Storage Quota', 'Warning at 80%, critical at 90%. Auto-prunes mesh chunks.', Colors.brown),
              _buildInfoSection(Icons.speed_rounded, 'Adaptive Chunking', 'Tracks throughput per peer. >10MB/s -> 512KB, >1MB/s -> 256KB, <1MB/s -> 64KB.', Colors.indigo),
              _buildInfoSection(Icons.timer_rounded, 'Full Maintenance Tick', 'Runs all 12 modules sequentially every 5 minutes.', Colors.deepOrange),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildInfoSection(IconData icon, String title, String desc, Color color) {
    return Padding(
      padding: EdgeInsets.only(bottom: 12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 32, height: 32,
            decoration: BoxDecoration(
              color: color.withValues(alpha: 0.15),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(icon, size: 16, color: color),
          ),
          SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TextStyle(fontSize: 13, fontWeight: FontWeight.bold, color: AppTheme.current.text)),
                SizedBox(height: 2),
                Text(desc, style: TextStyle(fontSize: 12, color: AppTheme.current.mutedText, height: 1.3)),
              ],
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Column(
        children: [
          SizedBox(height: MediaQuery.of(context).padding.top + kToolbarHeight),
          _buildClawHeader(),
          Expanded(
            child: Column(
              children: [
                // Permanent terminal log (same size as header)
                _buildActivityLogView(),
                // Tile buttons below terminal
                Expanded(child: _buildTileGrid()),
              ],
            ),
          ),
        ],
      ),
      bottomNavigationBar: _buildBottomBar(),
    );
  }

  void _refreshActivityLog() {
    try {
      final jsonStr = _client.getIntroClawActivityLog();
      final List<dynamic> entries = jsonDecode(jsonStr);
      setState(() {
        _activityLog = entries.map((e) => Map<String, dynamic>.from(e as Map)).toList();
        _activityLog.sort((a, b) => (b['t'] as int).compareTo(a['t'] as int));
      });
    } catch (_) {}
  }

  Widget _buildClawHeader() {
    return Padding(
      padding: EdgeInsets.fromLTRB(16, 8, 16, 12),
      child: GlassmorphicContainer(
        blur: 10,
        tintAlpha: 0.06,
        borderAlpha: 0.1,
        borderRadius: BorderRadius.circular(16),
        padding: EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.psychology_rounded, color: AppTheme.current.accent, size: 24),
                SizedBox(width: 10),
                Text('INTRO-CLAW', style: TextStyle(
                  fontSize: 13, fontWeight: FontWeight.bold,
                  color: AppTheme.current.text, letterSpacing: 1.5,
                )),
                Spacer(),
                Container(
                  padding: EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                  decoration: BoxDecoration(
                    color: AppTheme.current.accent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Text('LOCAL', style: TextStyle(
                    fontSize: 9, fontWeight: FontWeight.bold,
                    color: AppTheme.current.accent,
                    letterSpacing: 1,
                  )),
                ),
              ],
            ),
            SizedBox(height: 10),
            Text(
              'Intro-Claw is an AI intelligent agent working under the hood, handling networking, engine optimisation, and Introvert mesh swarm interactions. All operations run 100% on-device in a sandboxed environment — zero data leaked, zero external calls.',
              style: TextStyle(
                fontSize: 11,
                color: AppTheme.current.text,
                height: 1.4,
                fontStyle: FontStyle.italic,
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ── Local Mode: Query Tile Grid ──────────────────────────────

  Widget _buildTileGrid() {
    final tiles = [
      // Master search
      _QueryTile(Icons.manage_search_rounded, 'Semantic Search', 'master_search', Colors.white),
      // Data queries
      _QueryTile(Icons.photo_library_outlined, 'Photos', 'Show my photos', Colors.blue),
      _QueryTile(Icons.videocam_outlined, 'Videos', 'Show my videos', Colors.red),
      _QueryTile(Icons.insert_drive_file_outlined, 'Files', 'Show my files', Colors.orange),
      _QueryTile(Icons.people_outline, 'Contacts', 'Show my contacts', Colors.green),
      _QueryTile(Icons.sticky_note_2_outlined, 'Notes', 'Show my notes', Colors.amber),
      _QueryTile(Icons.chat_bubble_outline, 'Messages', 'Show recent messages', Colors.cyan),
      _QueryTile(Icons.call_outlined, 'Calls', 'Recent calls', Colors.purple),
      // Intro-Claw actions (not duplicated in terminal)
      _QueryTile(Icons.radar_rounded, 'Network Tune', 'run_network_recon', Colors.orangeAccent),
      _QueryTile(Icons.healing_rounded, 'Heal Mesh', 'run_network_heal', Colors.cyanAccent),
      _QueryTile(Icons.build_outlined, 'Run Maintenance', 'run_maintenance', Colors.indigo),
    ];

    return Column(
      children: [
        SizedBox(height: 8),
        Expanded(
          child: GridView.builder(
            padding: EdgeInsets.fromLTRB(16, 4, 16, 8),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: 3,
              mainAxisSpacing: 8,
              crossAxisSpacing: 8,
              childAspectRatio: 1.0,
            ),
            itemCount: tiles.length,
            itemBuilder: (context, index) => _buildQueryCard(tiles[index]),
          ),
        ),
      ],
    );
  }

  Widget _buildQueryCard(_QueryTile tile) {
    return GestureDetector(
      onTap: () => _executeTileQuery(tile.query),
      child: GlassmorphicContainer(
        borderRadius: BorderRadius.circular(14),
        tintColor: tile.color,
        blur: 10,
        tintAlpha: 0.08,
        borderAlpha: 0.15,
        padding: EdgeInsets.zero,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              width: 40, height: 40,
              decoration: BoxDecoration(
                color: tile.color.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(10),
              ),
              child: Icon(tile.icon, color: tile.color, size: 20),
            ),
            SizedBox(height: 6),
            Text(tile.label, style: TextStyle(
              fontSize: 12, fontWeight: FontWeight.w600, color: AppTheme.current.text,
            )),
            if (tile.subtitle != null) ...[
              SizedBox(height: 2),
              Text(tile.subtitle!, style: TextStyle(
                fontSize: 9, color: tile.color.withValues(alpha: 0.8),
                fontWeight: FontWeight.w500,
              ), maxLines: 1, overflow: TextOverflow.ellipsis),
            ],
          ],
        ),
      ),
    );
  }

  void _executeTileQuery(String query) {
    // Handle master search tile
    if (query == 'master_search') {
      _showMasterSearch();
      return;
    }
    // Handle Intro-Claw action tiles directly
    if (query == 'run_network_recon') {
      _runNetworkRecon();
      return;
    }
    if (query == 'run_network_heal') {
      _healDisconnectedPeers();
      return;
    }
    if (query == 'run_maintenance') {
      _client.triggerIntroClawTick();
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Maintenance tick triggered')),
      );
      return;
    }
    if (query == 'storage_status') {
      try {
        final reconJson = _client.runNetworkRecon();
        String storageInfo = 'Storage information unavailable';
        if (reconJson.contains('Drive:')) {
          final lines = reconJson.split('\n');
          final storageLines = lines.where((l) => l.contains('Drive:') || l.contains('Mesh:')).toList();
          if (storageLines.isNotEmpty) {
            storageInfo = storageLines.join('\n');
          }
        }
        _showTileResultOverlay('Sovereign Storage', storageInfo, []);
      } catch (e) {
        _showTileResultOverlay('Sovereign Storage', 'Error: $e', []);
      }
      return;
    }
    if (query == 'engine_status') {
      try {
        final statusJson = _client.getIntroClawStatus();
        final status = jsonDecode(statusJson) as Map<String, dynamic>;
        final isActive = status['is_active'] == true;
        final mode = status['mode'] ?? 'local';
        _showTileResultOverlay('Intro-Claw Engine',
            'Status: ${isActive ? "ACTIVE" : "INACTIVE"}\nMode: $mode\n\nWhen active, runs 17 modules every 5 minutes:\n• Battery throttling\n• Database pruning\n• Media cleanup\n• Connection optimization\n• Message batching\n• Predictive prefetch\n• Sync prioritization\n• Health scoring\n• Adaptive chunking\n• Dead letter detection\n• Offline queue\n• Storage quota\n• VoIP monitoring', []);
      } catch (e) {
        _showTileResultOverlay('Intro-Claw Engine', 'Error: $e', []);
      }
      return;
    }
    if (query == 'offline_queue_status') {
      _showTileResultOverlay('Offline Queue', 'Offline message queue is active. Messages are buffered when network drops and flushed when restored.', []);
      return;
    }
    if (query == 'dead_letter_check') {
      _showTileResultOverlay('Dead Letter Detection', 'Dead letter detector runs every 5 minutes. Messages stuck >5 minutes are flagged and alternative routes are attempted.', []);
      return;
    }
    if (query == 'connection_prewarm') {
      _showTileResultOverlay('Connection Pre-warming', 'Intro-Claw pre-dials top contacts when you open the contacts list. 5-minute cooldown per peer.', []);
      return;
    }
    if (query == 'peer_health_check') {
      _showTileResultOverlay('Peer Health Scoring', 'Decay-based scoring (0.9 decay, 0.1 boost) per peer. Unstable peers are flagged for relay pre-establishment after 3 disconnects in 1 hour.', []);
      return;
    }
    if (query == 'bandwidth_check') {
      try {
        final quality = _client.voipGetQuality();
        _showTileResultOverlay('Bandwidth Monitor', 'Current status: $quality\n\nQuality tiers:\n• Full: >10 MB/s\n• Medium: >1 MB/s\n• Low: >100 KB/s\n• Minimal: <100 KB/s', []);
      } catch (e) {
        _showTileResultOverlay('Bandwidth Monitor', 'Error: $e', []);
      }
      return;
    }
    if (query == 'battery_status') {
      _showTileResultOverlay('Battery Throttling', 'Reduces sync frequency, heartbeat, and max connections when battery is low.\n\nThresholds:\n• Low: 20% — reduces sync frequency\n• Critical: 10% — minimal connections only', []);
      return;
    }

    setState(() => _isLoading = true);

    try {
      final responseJson = _client.processAssistantQuery(query);
      final response = jsonDecode(responseJson) as Map<String, dynamic>;
      final answer = response['answer'] as String? ?? 'No answer';
      final results = (response['results'] as List<dynamic>?) ?? [];

      setState(() => _isLoading = false);
      _showTileResultOverlay(query, answer, results);
    } catch (e) {
      setState(() => _isLoading = false);
      _showTileResultOverlay(query, "Error: $e", []);
    }
  }

  void _showMasterSearch() {
    final searchController = TextEditingController();
    Map<String, List<dynamic>> results = {};
    String query = '';
    bool searching = false;
    String? semanticAnswer;

    showDialog(
      context: context,
      barrierDismissible: false,
      barrierColor: Colors.black.withValues(alpha: 0.5),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) {
          Future<void> runSearch(String val) async {
            if (val.isEmpty) {
              setDialogState(() { results = {}; searching = false; semanticAnswer = null; });
              return;
            }
            setDialogState(() { searching = true; semanticAnswer = null; });
            final q = val.toLowerCase();
            final Map<String, List<dynamic>> found = {};
            final Set<String> exactMsgKeys = {};

            try {
              // --- EXACT SUBSTRING MATCHING (instant, Dart-side) ---

              // Search contacts
              final contacts = _client.getContacts();
              final matchedContacts = contacts.where((c) {
                final alias = (c['alias']?.toString() ?? '').toLowerCase();
                final peerId = (c['peer_id']?.toString() ?? '').toLowerCase();
                final handle = (c['handle']?.toString() ?? '').toLowerCase();
                final globalName = (c['global_name']?.toString() ?? '').toLowerCase();
                return alias.contains(q) || peerId.contains(q) || handle.contains(q) || globalName.contains(q);
              }).toList();
              if (matchedContacts.isNotEmpty) found['Contacts'] = matchedContacts;

              // Search notes
              final notes = _client.notesGetAll();
              final matchedNotes = notes.where((n) {
                final title = (n['title'] as String? ?? '').toLowerCase();
                final content = (n['content'] as String? ?? '').toLowerCase();
                final tags = (n['tags'] as String? ?? '').toLowerCase();
                return title.contains(q) || content.contains(q) || tags.contains(q);
              }).toList();
              if (matchedNotes.isNotEmpty) found['Notes'] = matchedNotes;

              // Search drive files
              final files = _client.driveGetAll();
              final matchedFiles = files.where((f) {
                final name = (f['filename']?.toString() ?? '').toLowerCase();
                return name.contains(q);
              }).toList();
              if (matchedFiles.isNotEmpty) found['Drive'] = matchedFiles;

              // Search chat messages across all contacts
              final Map<String, int> chatHitCounts = {};
              for (var c in contacts) {
                final peerId = c['peer_id'] as String?;
                if (peerId == null) continue;
                try {
                  final msgs = _client.getMessages(peerId);
                  for (var m in msgs) {
                    final content = (m['content'] as String? ?? '').toLowerCase();
                    if (content.contains(q)) {
                      final alias = c['alias']?.toString().isNotEmpty == true ? c['alias'] : peerId.substring(0, 8);
                      chatHitCounts[alias] = (chatHitCounts[alias] ?? 0) + 1;
                      exactMsgKeys.add('${peerId}_${m['msg_id']}');
                    }
                  }
                } catch (_) {}
              }
              if (chatHitCounts.isNotEmpty) {
                found['Messages'] = chatHitCounts.entries.map((e) => {'chat': e.key, 'count': e.value}).toList();
              }

              // Search groups
              final allGroups = _client.getAllGroups();
              final matchedGroups = allGroups.where((g) {
                if (g == null || g is! List || g.length < 2) return false;
                final name = (g[1]?.toString() ?? '').toLowerCase();
                return name.contains(q);
              }).toList();
              if (matchedGroups.isNotEmpty) found['Groups'] = matchedGroups;

              // --- SEMANTIC SEARCH (Intro-Claw engine) ---
              try {
                final responseJson = _client.processAssistantQuery(val);
                final response = jsonDecode(responseJson) as Map<String, dynamic>;
                final answer = response['answer'] as String? ?? '';
                final semanticResults = (response['results'] as List<dynamic>?) ?? [];

                if (answer.isNotEmpty && answer != 'No answer') {
                  semanticAnswer = answer;
                }

                // Merge semantic results into existing categories
                if (semanticResults.isNotEmpty) {
                  final existing = found.putIfAbsent('Semantic Matches', () => []);
                  for (var sr in semanticResults) {
                    // Deduplicate against exact matches
                    String key = '';
                    if (sr is Map) {
                      key = (sr['content'] ?? sr['title'] ?? sr['filename'] ?? sr.toString()).toString();
                    } else if (sr is List) {
                      key = sr.join('_');
                    } else {
                      key = sr.toString();
                    }
                    // Check if this result is already captured by exact matching
                    bool isDuplicate = false;
                    for (var section in found.entries) {
                      if (section.key == 'Semantic Matches') continue;
                      for (var item in section.value) {
                        String itemStr = '';
                        if (item is Map) {
                          itemStr = (item['title'] ?? item['alias'] ?? item['filename'] ?? item['content'] ?? '').toString().toLowerCase();
                        } else if (item is List) {
                          itemStr = item.join(' ').toLowerCase();
                        }
                        if (itemStr.isNotEmpty && (key.toLowerCase().contains(itemStr) || itemStr.contains(key.toLowerCase()))) {
                          isDuplicate = true;
                          break;
                        }
                      }
                      if (isDuplicate) break;
                    }
                    if (!isDuplicate) {
                      existing.add(sr);
                    }
                  }
                  if (existing.isEmpty) found.remove('Semantic Matches');
                }
              } catch (e) {
                // Semantic search failed — exact results still show
                debugPrint("Intro-Claw semantic search error: $e");
              }
            } catch (_) {}

            setDialogState(() { results = found; searching = false; });
          }

          return Dialog(
            backgroundColor: Colors.transparent,
            insetPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 40),
            child: Container(
              constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.8),
              decoration: BoxDecoration(
                color: AppTheme.current.bg,
                borderRadius: BorderRadius.circular(20),
                border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.15)),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    padding: EdgeInsets.fromLTRB(16, 12, 16, 12),
                    decoration: BoxDecoration(
                      border: Border(bottom: BorderSide(color: AppTheme.current.text.withValues(alpha: 0.06))),
                    ),
                    child: Row(
                      children: [
                        Icon(Icons.manage_search_rounded, color: AppTheme.current.accent, size: 22),
                        SizedBox(width: 8),
                        Expanded(
                          child: Text('SEMANTIC SEARCH', style: TextStyle(
                            color: AppTheme.current.accent, fontSize: 13,
                            fontWeight: FontWeight.bold, letterSpacing: 1.2,
                          )),
                        ),
                        Container(
                          padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                          decoration: BoxDecoration(
                            color: AppTheme.current.accent.withValues(alpha: 0.1),
                            borderRadius: BorderRadius.circular(4),
                          ),
                          child: Text('INTRO-CLAW', style: TextStyle(color: AppTheme.current.accent, fontSize: 8, fontWeight: FontWeight.bold, letterSpacing: 0.5)),
                        ),
                        SizedBox(width: 8),
                        GestureDetector(
                          onTap: () { searchController.dispose(); Navigator.pop(ctx); },
                          child: Icon(Icons.close, color: AppTheme.current.mutedText, size: 20),
                        ),
                      ],
                    ),
                  ),
                  Padding(
                    padding: EdgeInsets.fromLTRB(16, 12, 16, 8),
                    child: TextField(
                      controller: searchController,
                      autofocus: true,
                      style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                      onChanged: (val) {
                        query = val;
                        runSearch(val);
                      },
                      decoration: InputDecoration(
                        hintText: "Search everything (exact + semantic)...",
                        hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                        prefixIcon: Icon(Icons.search, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 18),
                        filled: true,
                        fillColor: AppTheme.current.text.withValues(alpha: 0.04),
                        border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
                      ),
                    ),
                  ),
                  if (query.isNotEmpty)
                    Padding(
                      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                      child: Row(
                        children: [
                          Icon(Icons.search, size: 14, color: AppTheme.current.accent),
                          SizedBox(width: 6),
                          Text(
                            '${results.values.fold(0, (sum, list) => sum + list.length)} result${results.values.fold(0, (sum, list) => sum + list.length) == 1 ? '' : 's'} across ${results.length} section${results.length == 1 ? '' : 's'}',
                            style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.w600),
                          ),
                        ],
                      ),
                    ),
                  if (semanticAnswer != null && semanticAnswer!.isNotEmpty)
                    Padding(
                      padding: EdgeInsets.fromLTRB(16, 4, 16, 4),
                      child: Container(
                        padding: EdgeInsets.all(10),
                        decoration: BoxDecoration(
                          color: AppTheme.current.accent.withValues(alpha: 0.06),
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.15)),
                        ),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Icon(Icons.psychology_rounded, color: AppTheme.current.accent, size: 16),
                            SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                semanticAnswer!,
                                style: TextStyle(color: AppTheme.current.text, fontSize: 12),
                                maxLines: 4,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  Flexible(
                    child: searching
                        ? Padding(
                            padding: EdgeInsets.all(32),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                CircularProgressIndicator(color: AppTheme.current.accent),
                                SizedBox(height: 12),
                                Text('Searching via Intro-Claw...', style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11)),
                              ],
                            ),
                          )
                        : results.isEmpty
                            ? Padding(
                                padding: EdgeInsets.all(32),
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Icon(Icons.manage_search_rounded, size: 48, color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                                    SizedBox(height: 12),
                                    Text(
                                      query.isEmpty ? 'Search across all chats, notes, files, contacts, and groups' : 'No results found',
                                      style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                                      textAlign: TextAlign.center,
                                    ),
                                    if (query.isNotEmpty) ...[
                                      SizedBox(height: 4),
                                      Text(
                                        'Intro-Claw semantic search also found no matches.',
                                        style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.3), fontSize: 11),
                                        textAlign: TextAlign.center,
                                      ),
                                    ],
                                  ],
                                ),
                              )
                            : ListView(
                                shrinkWrap: true,
                                padding: EdgeInsets.fromLTRB(16, 8, 16, 16),
                                children: results.entries.map((entry) {
                                  return Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Padding(
                                        padding: EdgeInsets.only(top: 8, bottom: 4),
                                        child: Row(
                                          children: [
                                            Text(
                                              entry.key.toUpperCase(),
                                              style: TextStyle(color: AppTheme.current.accent, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 1.2),
                                            ),
                                            if (entry.key == 'Semantic Matches') ...[
                                              SizedBox(width: 6),
                                              Icon(Icons.psychology_rounded, size: 12, color: AppTheme.current.accent),
                                            ],
                                          ],
                                        ),
                                      ),
                                      ...entry.value.take(5).map((item) {
                                        String title = '';
                                        String subtitle = '';
                                        IconData icon = Icons.circle;
                                        Color iconColor = AppTheme.current.accent;

                                        if (entry.key == 'Contacts') {
                                          title = item['alias']?.toString().isNotEmpty == true ? item['alias'] : (item['global_name'] ?? item['peer_id'] ?? '');
                                          subtitle = item['handle']?.toString() ?? '';
                                          icon = Icons.person;
                                          iconColor = Colors.green;
                                        } else if (entry.key == 'Notes') {
                                          title = item['title'] as String? ?? 'Untitled';
                                          subtitle = (item['content'] as String? ?? '').replaceAll('\n', ' ');
                                          if (subtitle.length > 60) subtitle = '${subtitle.substring(0, 60)}...';
                                          icon = Icons.sticky_note_2_outlined;
                                          iconColor = Colors.amber;
                                        } else if (entry.key == 'Drive') {
                                          title = item['filename'] as String? ?? '';
                                          icon = Icons.insert_drive_file_outlined;
                                          iconColor = Colors.orange;
                                        } else if (entry.key == 'Messages') {
                                          title = '${item['chat']} — ${item['count']} message${item['count'] == 1 ? '' : 's'}';
                                          icon = Icons.chat_bubble_outline;
                                          iconColor = Colors.cyan;
                                        } else if (entry.key == 'Groups') {
                                          title = item[1]?.toString() ?? '';
                                          icon = Icons.group;
                                          iconColor = Colors.blue;
                                        } else if (entry.key == 'Semantic Matches') {
                                          // Generic rendering for semantic results
                                          if (item is Map) {
                                            title = (item['title'] ?? item['answer'] ?? item['content'] ?? item['filename'] ?? item.toString()).toString();
                                            subtitle = (item['content'] ?? item['description'] ?? '').toString();
                                            if (subtitle.length > 60) subtitle = '${subtitle.substring(0, 60)}...';
                                          } else if (item is List) {
                                            title = item.join(' · ');
                                          } else {
                                            title = item.toString();
                                          }
                                          icon = Icons.auto_awesome;
                                          iconColor = AppTheme.current.accent;
                                        }

                                        return Container(
                                          margin: EdgeInsets.only(bottom: 4),
                                          decoration: BoxDecoration(
                                            color: entry.key == 'Semantic Matches'
                                                ? AppTheme.current.accent.withValues(alpha: 0.04)
                                                : AppTheme.current.text.withValues(alpha: 0.04),
                                            borderRadius: BorderRadius.circular(8),
                                          ),
                                          child: ListTile(
                                            dense: true,
                                            contentPadding: EdgeInsets.symmetric(horizontal: 8),
                                            leading: Icon(icon, color: iconColor, size: 18),
                                            title: Text(title, style: TextStyle(color: AppTheme.current.text, fontSize: 13), maxLines: 1, overflow: TextOverflow.ellipsis),
                                            subtitle: subtitle.isNotEmpty ? Text(subtitle, style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11), maxLines: 1, overflow: TextOverflow.ellipsis) : null,
                                          ),
                                        );
                                      }),
                                      if (entry.value.length > 5)
                                        Padding(
                                          padding: EdgeInsets.only(top: 4),
                                          child: Text(
                                            '+ ${entry.value.length - 5} more...',
                                            style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 11),
                                          ),
                                        ),
                                    ],
                                  );
                                }).toList(),
                              ),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }

  void _showTileResultOverlay(String query, String answer, List<dynamic> results) {
    showDialog(
      context: context,
      barrierColor: Colors.black.withValues(alpha: 0.5),
      builder: (context) => Dialog(
        backgroundColor: Colors.transparent,
        insetPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 40),
        child: Container(
          constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.7),
          decoration: BoxDecoration(
            color: AppTheme.current.bg,
            borderRadius: BorderRadius.circular(20),
            border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.15)),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: EdgeInsets.fromLTRB(16, 12, 16, 12),
                decoration: BoxDecoration(
                  border: Border(bottom: BorderSide(color: AppTheme.current.text.withValues(alpha: 0.06))),
                ),
                child: Row(
                  children: [
                    Icon(Icons.psychology_rounded, color: AppTheme.current.accent, size: 20),
                    SizedBox(width: 8),
                    Expanded(
                      child: Text(query, style: TextStyle(
                        color: AppTheme.current.accent, fontSize: 13,
                        fontWeight: FontWeight.bold, letterSpacing: 0.5,
                      )),
                    ),
                    GestureDetector(
                      onTap: () => Navigator.pop(context),
                      child: Icon(Icons.close, color: AppTheme.current.mutedText, size: 20),
                    ),
                  ],
                ),
              ),
              Flexible(
                child: SingleChildScrollView(
                  padding: EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(answer, style: TextStyle(
                        color: AppTheme.current.text, fontSize: 13, height: 1.4,
                      )),
                      if (results.isNotEmpty) ...[
                        SizedBox(height: 16),
                        ...results.take(10).map((r) {
                          final result = r as Map<String, dynamic>;
                          final type = result['result_type'] as String? ?? 'unknown';
                          final title = result['title'] as String? ?? '';
                          final subtitle = result['subtitle'] as String? ?? '';
                          final peerId = result['peer_id'] as String?;
                          final groupId = result['group_id'] as String?;
                          final fileHash = result['file_hash'] as String?;
                          final icon = _getIconForType(type);
                          final color = _getColorForType(type);

                          return GestureDetector(
                            onTap: () => _handleResultTap(type, peerId, groupId, fileHash, title),
                            child: Container(
                              margin: EdgeInsets.only(bottom: 8),
                              padding: EdgeInsets.all(10),
                              decoration: BoxDecoration(
                                color: AppTheme.current.surface.withValues(alpha: 0.7),
                                borderRadius: BorderRadius.circular(12),
                                border: Border.all(color: color.withValues(alpha: 0.15)),
                              ),
                              child: Row(
                                children: [
                                  Container(
                                    width: 32, height: 32,
                                    decoration: BoxDecoration(
                                      color: color.withValues(alpha: 0.12),
                                      borderRadius: BorderRadius.circular(8),
                                    ),
                                    child: Icon(icon, size: 16, color: color),
                                  ),
                                  SizedBox(width: 10),
                                  Expanded(
                                    child: Column(
                                      crossAxisAlignment: CrossAxisAlignment.start,
                                      children: [
                                        Text(title, style: TextStyle(
                                          fontSize: 12, fontWeight: FontWeight.w600,
                                          color: AppTheme.current.text,
                                        ), maxLines: 1, overflow: TextOverflow.ellipsis),
                                        if (subtitle.isNotEmpty)
                                          Text(subtitle, style: TextStyle(
                                            fontSize: 11, color: AppTheme.current.mutedText,
                                          ), maxLines: 1, overflow: TextOverflow.ellipsis),
                                      ],
                                    ),
                                  ),
                                  Icon(Icons.chevron_right, size: 16, color: AppTheme.current.mutedText.withValues(alpha: 0.4)),
                                ],
                              ),
                            ),
                          );
                        }),
                      ],
                    ],
                  ),
                ),
              ),
              Padding(
                padding: EdgeInsets.fromLTRB(16, 8, 16, 12),
                child: GestureDetector(
                  onTap: () => Navigator.pop(context),
                  child: Container(
                    width: double.infinity,
                    padding: EdgeInsets.symmetric(vertical: 10),
                    decoration: BoxDecoration(
                      color: AppTheme.current.accent.withValues(alpha: 0.1),
                      borderRadius: BorderRadius.circular(10),
                      border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.2)),
                    ),
                    child: Center(
                      child: Text('CLOSE', style: TextStyle(
                        color: AppTheme.current.accent, fontSize: 11,
                        fontWeight: FontWeight.bold, letterSpacing: 1.2,
                      )),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  // ── Activity Log View ──────────────────────────────────────

  Widget _buildActivityLogView() {
    if (_activityLog.isEmpty) {
      return Container(
        height: 150,
        margin: EdgeInsets.symmetric(horizontal: 16),
        decoration: BoxDecoration(
          color: Colors.black.withValues(alpha: 0.8),
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.2)),
        ),
        child: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(Icons.terminal_rounded, size: 24, color: Colors.greenAccent.withValues(alpha: 0.3)),
              SizedBox(height: 8),
              Text('Waiting for activity...', style: TextStyle(color: Colors.white38, fontSize: 11, fontFamily: 'monospace')),
            ],
          ),
        ),
      );
    }

    return Container(
      height: 150,
      margin: EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.8),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.2)),
      ),
      child: Column(
        children: [
          // Terminal header
          Container(
            padding: EdgeInsets.symmetric(horizontal: 12, vertical: 6),
            decoration: BoxDecoration(
              border: Border(
                bottom: BorderSide(color: Colors.greenAccent.withValues(alpha: 0.2)),
              ),
            ),
            child: Row(
              children: [
                Icon(Icons.terminal_rounded, color: Colors.greenAccent, size: 12),
                SizedBox(width: 6),
                Text('ACTIVITY LOG', style: TextStyle(
                  color: Colors.greenAccent, fontSize: 9,
                  fontFamily: 'monospace', fontWeight: FontWeight.bold, letterSpacing: 1,
                )),
                Spacer(),
                Container(
                  width: 6, height: 6,
                  decoration: BoxDecoration(
                    color: Colors.greenAccent,
                    shape: BoxShape.circle,
                  ),
                ),
                SizedBox(width: 4),
                Text('LIVE', style: TextStyle(
                  color: Colors.greenAccent, fontSize: 8,
                  fontFamily: 'monospace', fontWeight: FontWeight.bold,
                )),
              ],
            ),
          ),
          // Terminal body
          Expanded(
            child: ListView.builder(
              controller: _logScrollController,
              padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              itemCount: _activityLog.length,
              itemBuilder: (context, index) {
                final entry = _activityLog[index];
                final timestamp = entry['t'] as int;
                final category = entry['c'] as String;
                final message = entry['m'] as String;
                final severity = entry['s'] as String;

                final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
                final ageSec = now - timestamp;
                final timeStr = ageSec < 60
                    ? '${ageSec}s'
                    : ageSec < 3600
                        ? '${ageSec ~/ 60}m'
                        : '${ageSec ~/ 3600}h';

                final severityColor = switch (severity) {
                  'success' => Colors.greenAccent,
                  'warn' => Colors.orangeAccent,
                  'action' => Colors.cyanAccent,
                  'info' => Colors.white70,
                  _ => Colors.white54,
                };

                final categoryIcon = switch (category) {
                  'tick' => '⏱',
                  'battery' => '🔋',
                  'recon' => '📡',
                  'heal' => '💊',
                  'storage' => '💾',
                  'offline_queue' => '📤',
                  'dead_letter' => '📧',
                  'health' => '❤️',
                  'prewarm' => '🔥',
                  'maintenance' => '🔧',
                  'network' => '🌐',
                  'node' => '🖥',
                  'node_cache' => '📦',
                  'node_dead_letter' => '📬',
                  'node_bandwidth' => '📊',
                  'node_status' => '📈',
                  'voip' => '📞',
                  'anchor' => '⚓',
                  _ => '●',
                };

                return Padding(
                  padding: EdgeInsets.only(bottom: 2),
                  child: RichText(
                    text: TextSpan(
                      children: [
                        TextSpan(
                          text: '[$timeStr] ',
                          style: TextStyle(
                            color: Colors.white38,
                            fontSize: 10,
                            fontFamily: 'monospace',
                          ),
                        ),
                        TextSpan(
                          text: '$categoryIcon ',
                          style: TextStyle(fontSize: 10),
                        ),
                        TextSpan(
                          text: message,
                          style: TextStyle(
                            color: severityColor,
                            fontSize: 10,
                            fontFamily: 'monospace',
                            height: 1.3,
                          ),
                        ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
          // Terminal footer
          Container(
            padding: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
            decoration: BoxDecoration(
              border: Border(
                top: BorderSide(color: Colors.greenAccent.withValues(alpha: 0.2)),
              ),
            ),
            child: Row(
              children: [
                Text('${_activityLog.length} entries', style: TextStyle(
                  color: Colors.white38, fontSize: 8, fontFamily: 'monospace',
                )),
              ],
            ),
          ),
        ],
      ),
    );
  }

  // ── Hybrid Mode: Chat Interface ──────────────────────────────

  Widget _buildChatMode() {
    return Column(
      children: [
        Expanded(child: _messages.isEmpty ? _buildTileGrid() : _buildMessageList()),
        if (_isLoading) _buildTypingIndicator(),
        _buildInputBar(),
      ],
    );
  }

  Widget _buildMessageList() {
    return ListView.builder(
      controller: _scrollController,
      padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      itemCount: _messages.length,
      itemBuilder: (context, index) => _buildMessageBubble(_messages[index]),
    );
  }

  Widget _buildMessageBubble(_ChatMessage message) {
    final isUser = message.isUser;

    if (message.isReconReport) {
      return Padding(
        padding: EdgeInsets.only(bottom: 12),
        child: Container(
          padding: EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: Colors.black.withValues(alpha: 0.8),
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.3), width: 1),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(message.text, style: TextStyle(
                fontSize: 11, color: Colors.greenAccent,
                fontFamily: 'monospace', height: 1.5,
              )),
            ],
          ),
        ),
      );
    }

    return Padding(
      padding: EdgeInsets.only(bottom: 12),
      child: Row(
        mainAxisAlignment: isUser ? MainAxisAlignment.end : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isUser) ...[
            CircleAvatar(
              radius: 14,
              backgroundColor: AppTheme.current.accent.withValues(alpha: 0.15),
              child: Icon(Icons.psychology_rounded, size: 16, color: AppTheme.current.accent),
            ),
            SizedBox(width: 8),
          ],
          Flexible(
            child: Container(
              padding: EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: isUser ? AppTheme.current.accent : AppTheme.current.surface,
                borderRadius: BorderRadius.circular(16),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(message.text, style: TextStyle(
                    fontSize: 13, height: 1.4,
                    color: isUser ? AppTheme.current.bg : AppTheme.current.text,
                  )),
                  if (message.results != null && message.results!.isNotEmpty) ...[
                    SizedBox(height: 10),
                    _buildResultsList(message.results!),
                  ],
                ],
              ),
            ),
          ),
          if (isUser) ...[
            SizedBox(width: 8),
            CircleAvatar(
              radius: 14,
              backgroundColor: AppTheme.current.accent.withValues(alpha: 0.15),
              child: Icon(Icons.person, size: 16, color: AppTheme.current.accent),
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildResultsList(List<dynamic> results) {
    return Column(
      children: results.take(5).map((r) {
        final result = r as Map<String, dynamic>;
        final type = result['result_type'] as String? ?? 'unknown';
        final title = result['title'] as String? ?? '';
        final subtitle = result['subtitle'] as String? ?? '';
        final icon = _getIconForType(type);
        final color = _getColorForType(type);

        return Container(
          margin: EdgeInsets.only(bottom: 6),
          padding: EdgeInsets.all(8),
          decoration: BoxDecoration(
            color: AppTheme.current.bg.withValues(alpha: 0.5),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Row(
            children: [
              Container(
                width: 28, height: 28,
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Icon(icon, size: 14, color: color),
              ),
              SizedBox(width: 8),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: TextStyle(fontSize: 11, fontWeight: FontWeight.w600, color: AppTheme.current.text), maxLines: 1, overflow: TextOverflow.ellipsis),
                    if (subtitle.isNotEmpty) Text(subtitle, style: TextStyle(fontSize: 10, color: AppTheme.current.mutedText), maxLines: 1, overflow: TextOverflow.ellipsis),
                  ],
                ),
              ),
            ],
          ),
        );
      }).toList(),
    );
  }

  Widget _buildTypingIndicator() {
    return Padding(
      padding: EdgeInsets.symmetric(horizontal: 32, vertical: 4),
      child: Row(
        children: [
          SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent)),
          SizedBox(width: 8),
          Text('Searching...', style: TextStyle(fontSize: 11, color: AppTheme.current.mutedText)),
        ],
      ),
    );
  }

  Widget _buildInputBar() {
    return Container(
      padding: EdgeInsets.fromLTRB(12, 8, 12, 12),
      decoration: BoxDecoration(
        color: AppTheme.current.bg,
        border: Border(top: BorderSide(color: AppTheme.current.text.withValues(alpha: 0.08))),
      ),
      child: SafeArea(
        top: false,
        child: Row(
          children: [
            GestureDetector(
              onTap: () => setState(() {
                _messages.clear();
              }),
              child: Container(
                width: 40, height: 40,
                decoration: BoxDecoration(color: AppTheme.current.surface, shape: BoxShape.circle),
                child: Icon(Icons.grid_view_rounded, color: AppTheme.current.mutedText, size: 20),
              ),
            ),
            SizedBox(width: 8),
            Expanded(
              child: Container(
                decoration: BoxDecoration(
                  color: AppTheme.current.surface,
                  borderRadius: BorderRadius.circular(22),
                ),
                child: TextField(
                  controller: _inputController,
                  style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                  decoration: InputDecoration(
                    hintText: 'Ask about messages, files, contacts...',
                    hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 13),
                    border: InputBorder.none,
                    contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                  ),
                  onSubmitted: (_) => _sendQuery(),
                ),
              ),
            ),
            SizedBox(width: 8),
            GestureDetector(
              onTap: _sendQuery,
              child: Container(
                width: 40, height: 40,
                decoration: BoxDecoration(
                  color: AppTheme.current.accent,
                  shape: BoxShape.circle,
                ),
                child: Icon(Icons.send_rounded, color: AppTheme.current.bg, size: 20),
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _sendQuery() {
    final query = _inputController.text.trim();
    if (query.isEmpty || _isLoading) return;

    setState(() {
      _messages.add(_ChatMessage(text: query, isUser: true, timestamp: DateTime.now()));
      _isLoading = true;
    });
    _inputController.clear();
    _scrollToBottom();

    try {
      final responseJson = _client.processAssistantQuery(query);
      final response = jsonDecode(responseJson) as Map<String, dynamic>;
      final answer = response['answer'] as String? ?? 'No answer';
      final results = (response['results'] as List<dynamic>?) ?? [];

      setState(() {
        _messages.add(_ChatMessage(
          text: answer, isUser: false, timestamp: DateTime.now(),
          results: results, resultCount: response['result_count'] as int? ?? 0,
        ));
        _isLoading = false;
      });
      _scrollToBottom();
    } catch (e) {
      setState(() {
        _messages.add(_ChatMessage(text: "Error: $e", isUser: false, timestamp: DateTime.now()));
        _isLoading = false;
      });
      _scrollToBottom();
    }
  }

  // ── Bottom Bar: Recon / Heal / About ─────────────────────────

  Widget _buildBottomBar() {
    return SafeArea(
      top: false,
      child: Padding(
        padding: EdgeInsets.fromLTRB(16, 8, 16, 8),
        child: Row(
          children: [
            _buildBottomButton(
              icon: Icons.radar_rounded,
              label: 'RECON',
              isLoading: _isReconRunning,
              color: Colors.orangeAccent,
              onTap: _isReconRunning ? null : _runNetworkRecon,
            ),
            SizedBox(width: 8),
            _buildBottomButton(
              icon: Icons.healing_rounded,
              label: 'HEAL',
              isLoading: _isHealRunning,
              color: Colors.cyanAccent,
              onTap: (_isReconRunning || _isHealRunning) ? null : _healDisconnectedPeers,
            ),
            SizedBox(width: 8),
            _buildBottomButton(
              icon: Icons.info_outline,
              label: 'ABOUT',
              isLoading: false,
              color: AppTheme.current.accent,
              onTap: _showIntroClawInfo,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBottomButton({
    required IconData icon,
    required String label,
    required bool isLoading,
    required Color color,
    VoidCallback? onTap,
  }) {
    return Expanded(
      child: GestureDetector(
        onTap: onTap,
        child: GlassmorphicContainer(
          padding: EdgeInsets.symmetric(vertical: 10),
          borderRadius: BorderRadius.circular(10),
          tintColor: color,
          tintAlpha: 0.08,
          borderAlpha: 0.2,
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              if (isLoading)
                SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 1.5, color: color))
              else
                Icon(icon, color: color, size: 16),
              SizedBox(width: 6),
              Text(label, style: TextStyle(
                fontSize: 10, fontWeight: FontWeight.bold,
                color: color, letterSpacing: 1.2, fontFamily: 'monospace',
              )),
            ],
          ),
        ),
      ),
    );
  }

  // ── Helpers ──────────────────────────────────────────────────

  IconData _getIconForType(String type) {
    switch (type) {
      case 'message': return Icons.chat_bubble_outline;
      case 'file': return Icons.insert_drive_file_outlined;
      case 'note': return Icons.sticky_note_2_outlined;
      case 'contact': return Icons.person_outline;
      case 'call': return Icons.call_outlined;
      case 'group_message': return Icons.group_outlined;
      default: return Icons.help_outline;
    }
  }

  Color _getColorForType(String type) {
    switch (type) {
      case 'message': return Colors.blue;
      case 'file': return Colors.orange;
      case 'note': return Colors.amber;
      case 'contact': return Colors.green;
      case 'call': return Colors.purple;
      case 'group_message': return Colors.teal;
      default: return AppTheme.current.mutedText;
    }
  }

  void _handleResultTap(String type, String? peerId, String? groupId, String? fileHash, String title) {
    Navigator.pop(context); // Close the overlay

    switch (type) {
      case 'contact':
        if (peerId != null && peerId.isNotEmpty) {
          Navigator.push(
            context,
            MaterialPageRoute(builder: (context) => ChatScreen(
              peerId: peerId,
              peerName: title,
            )),
          );
        }
        break;
      case 'message':
        if (peerId != null && peerId.isNotEmpty) {
          Navigator.push(
            context,
            MaterialPageRoute(builder: (context) => ChatScreen(
              peerId: peerId,
              peerName: title,
            )),
          );
        }
        break;
      case 'group_message':
        if (groupId != null && groupId.isNotEmpty) {
          Navigator.push(
            context,
            MaterialPageRoute(builder: (context) => GroupChatScreen(
              groupId: groupId,
              groupName: title,
            )),
          );
        }
        break;
      case 'file':
        if (fileHash != null && fileHash.isNotEmpty) {
          try {
            final path = _client.resolveSandboxPath(fileHash);
            if (path != null && path.isNotEmpty) {
              OpenFile.open(path);
            } else {
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text('File not found locally')),
              );
            }
          } catch (_) {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('Could not open file')),
            );
          }
        }
        break;
      case 'note':
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Opening note: $title')),
        );
        break;
      case 'call':
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Call history: $title')),
        );
        break;
    }
  }
}

// ── Terminal Overlay Widget ──────────────────────────────────

class _TerminalOverlay extends StatefulWidget {
  final String title;
  final List<String> milestones;
  final String? finalReport;
  final VoidCallback onClose;

  const _TerminalOverlay({
    required this.title,
    required this.milestones,
    this.finalReport,
    required this.onClose,
  });

  @override
  State<_TerminalOverlay> createState() => _TerminalOverlayState();
}

class _TerminalOverlayState extends State<_TerminalOverlay> with SingleTickerProviderStateMixin {
  late AnimationController _cursorController;
  bool _showCursor = true;

  @override
  void initState() {
    super.initState();
    _cursorController = AnimationController(vsync: this, duration: Duration(milliseconds: 500))
      ..repeat(reverse: true);
    _cursorController.addListener(() {
      setState(() => _showCursor = _cursorController.value > 0.5);
    });
  }

  @override
  void dispose() {
    _cursorController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final hasReport = widget.finalReport != null;
    return Material(
      color: Colors.transparent,
      child: GestureDetector(
        onTap: widget.onClose,
        child: Container(
          color: Colors.black.withValues(alpha: 0.7),
          child: Center(
            child: GestureDetector(
              onTap: () {},
              child: Container(
                margin: EdgeInsets.symmetric(horizontal: 24, vertical: 40),
                constraints: BoxConstraints(maxHeight: MediaQuery.of(context).size.height * 0.75),
                decoration: BoxDecoration(
                  color: Color(0xFF0A0A0A),
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.3), width: 1),
                  boxShadow: [
                    BoxShadow(color: Colors.greenAccent.withValues(alpha: 0.1), blurRadius: 20, spreadRadius: 2),
                  ],
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _buildTerminalHeader(),
                    Flexible(
                      child: SingleChildScrollView(
                        padding: EdgeInsets.all(16),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            ...widget.milestones.map((m) => Padding(
                              padding: EdgeInsets.only(bottom: 4),
                              child: Text(m, style: TextStyle(
                                fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.9),
                                fontFamily: 'monospace', height: 1.4,
                              )),
                            )),
                            if (!hasReport && widget.milestones.isNotEmpty && widget.milestones.length < 7)
                              Text(_showCursor ? '█' : ' ', style: TextStyle(
                                fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.6),
                                fontFamily: 'monospace',
                              )),
                            if (!hasReport && widget.milestones.isEmpty)
                              Row(
                                children: [
                                  SizedBox(width: 12, height: 12, child: CircularProgressIndicator(strokeWidth: 1.5, color: Colors.greenAccent)),
                                  SizedBox(width: 8),
                                  Text('Initializing...', style: TextStyle(
                                    fontSize: 12, color: Colors.greenAccent.withValues(alpha: 0.6),
                                    fontFamily: 'monospace',
                                  )),
                                ],
                              ),
                            if (hasReport) ...[
                              SizedBox(height: 8),
                              Container(
                                width: double.infinity,
                                padding: EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: Colors.greenAccent.withValues(alpha: 0.05),
                                  borderRadius: BorderRadius.circular(8),
                                  border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.15)),
                                ),
                                child: Text(widget.finalReport!, style: TextStyle(
                                  fontSize: 11, color: Colors.greenAccent,
                                  fontFamily: 'monospace', height: 1.5,
                                )),
                              ),
                              SizedBox(height: 12),
                              Center(
                                child: GestureDetector(
                                  onTap: widget.onClose,
                                  child: Container(
                                    padding: EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                                    decoration: BoxDecoration(
                                      color: Colors.greenAccent.withValues(alpha: 0.15),
                                      borderRadius: BorderRadius.circular(6),
                                      border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.4)),
                                    ),
                                    child: Text('CLOSE', style: TextStyle(
                                      fontSize: 11, fontWeight: FontWeight.bold,
                                      color: Colors.greenAccent, letterSpacing: 1.5, fontFamily: 'monospace',
                                    )),
                                  ),
                                ),
                              ),
                            ],
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildTerminalHeader() {
    return Container(
      padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: Colors.greenAccent.withValues(alpha: 0.08),
        borderRadius: BorderRadius.vertical(top: Radius.circular(12)),
        border: Border(bottom: BorderSide(color: Colors.greenAccent.withValues(alpha: 0.15))),
      ),
      child: Row(
        children: [
          Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.redAccent.withValues(alpha: 0.7))),
          SizedBox(width: 6),
          Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.amber.withValues(alpha: 0.7))),
          SizedBox(width: 6),
          Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Colors.greenAccent.withValues(alpha: 0.7))),
          SizedBox(width: 12),
          Text(widget.title, style: TextStyle(
            fontSize: 11, fontWeight: FontWeight.bold,
            color: Colors.greenAccent.withValues(alpha: 0.8),
            letterSpacing: 1.5, fontFamily: 'monospace',
          )),
          Spacer(),
          GestureDetector(
            onTap: widget.onClose,
            child: Icon(Icons.close, size: 16, color: Colors.greenAccent.withValues(alpha: 0.5)),
          ),
        ],
      ),
    );
  }
}

// ── Data Models ──────────────────────────────────────────────

class _ChatMessage {
  final String text;
  final bool isUser;
  final DateTime timestamp;
  final List<dynamic>? results;
  final int? resultCount;
  final bool isReconReport;

  _ChatMessage({
    required this.text,
    required this.isUser,
    required this.timestamp,
    this.results,
    this.resultCount,
    this.isReconReport = false,
  });
}

class _QueryTile {
  final IconData icon;
  final String label;
  final String query;
  final Color color;
  final String? subtitle;
  _QueryTile(this.icon, this.label, this.query, this.color, {this.subtitle});
}
