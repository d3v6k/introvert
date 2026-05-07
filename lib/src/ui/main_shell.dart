import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../native/introvert_client.dart';
import '../../views/chat_screen.dart';
import 'widgets/rewards_hud.dart';

/// WhatsApp-style High-Performance Prototype UI.
/// Implements a polished Material 3 messaging layout with Chats, Calls, and Settings.
class MainShell extends StatefulWidget {
  const MainShell({super.key});

  @override
  State<MainShell> createState() => _MainShellState();
}

class _MainShellState extends State<MainShell> {
  int _selectedIndex = 0;
  final PageController _pageController = PageController();
  late final StreamSubscription<NetworkEvent> _networkSubscription;
  
  String _localStatus = "OFFLINE";
  Color _localStatusColor = Colors.redAccent;

  final List<Widget> _tabs = [
    const ChatsTab(),
    const CallsTab(),
    const SettingsTab(),
  ];

  @override
  void initState() {
    super.initState();
    _startGlobalListener();
  }

  void _startGlobalListener() {
    final client = IntrovertClient();
    client.startNetwork();
    _networkSubscription = client.networkStream.listen((event) {
      if (event.type == 2 || event.type == 4) {
        // Global Message Arrival (Handled by storage in core, but we can notify UI)
        debugPrint("Global Message Received: ${String.fromCharCodes(event.data)}");
      } else if (event.type == 10) {
        // Event 10: Local Node Status
        final status = event.data[0];
        setState(() {
          if (status == 1) {
            _localStatus = "ONLINE";
            _localStatusColor = Colors.greenAccent;
          } else if (status == 2) {
            _localStatus = "RELAY ACTIVE";
            _localStatusColor = Colors.orangeAccent;
          } else {
            _localStatus = "OFFLINE";
            _localStatusColor = Colors.redAccent;
          }
        });
      }
    });
  }

  @override
  void dispose() {
    _networkSubscription.cancel();
    _pageController.dispose();
    super.dispose();
  }

  void _onDestinationSelected(int index) {
    setState(() => _selectedIndex = index);
    _pageController.animateToPage(
      index,
      duration: const Duration(milliseconds: 400),
      curve: Curves.easeInOutCubic,
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Row(
          children: [
            const Text('Introvert', style: TextStyle(fontWeight: FontWeight.bold)),
            const SizedBox(width: 12),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
              decoration: BoxDecoration(
                color: _localStatusColor.withValues(alpha: 0.1),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: _localStatusColor.withValues(alpha: 0.3), width: 0.5),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(shape: BoxShape.circle, color: _localStatusColor),
                  ),
                  const SizedBox(width: 6),
                  Text(
                    _localStatus,
                    style: TextStyle(
                      fontSize: 9, 
                      fontWeight: FontWeight.bold, 
                      color: _localStatusColor,
                      letterSpacing: 0.5,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.sync_rounded), 
            onPressed: () => IntrovertClient().fetchMailbox(),
            tooltip: "Force P2P Sync",
          ),
          IconButton(
            icon: const Icon(Icons.refresh), 
            onPressed: () {
              // This is a bit of a hack to refresh the currently visible tab if it's ChatsTab
              // In a full implementation, we'd use a shared state or event bus.
              _pageController.notifyListeners(); 
            },
            tooltip: "Refresh View",
          ),
        ],
      ),
      body: PageView(
        controller: _pageController,
        onPageChanged: (index) => setState(() => _selectedIndex = index),
        children: _tabs,
      ),
      bottomNavigationBar: Container(
        decoration: BoxDecoration(
          border: const Border(top: BorderSide(color: Colors.white10, width: 0.5)),
          color: const Color(0xFF0A0E17).withValues(alpha: 0.8),
        ),
        child: NavigationBar(
          selectedIndex: _selectedIndex,
          onDestinationSelected: _onDestinationSelected,
          backgroundColor: Colors.transparent,
          indicatorColor: const Color(0xFF00FFFF).withValues(alpha: 0.1),
          destinations: const [
            NavigationDestination(
              icon: Icon(Icons.chat_bubble_outline, color: Colors.white54),
              selectedIcon: Icon(Icons.chat_bubble, color: Color(0xFF00FFFF)),
              label: 'CHATS',
            ),
            NavigationDestination(
              icon: Icon(Icons.call_outlined, color: Colors.white54),
              selectedIcon: Icon(Icons.call, color: Color(0xFF00FFFF)),
              label: 'CALLS',
            ),
            NavigationDestination(
              icon: Icon(Icons.settings_outlined, color: Colors.white54),
              selectedIcon: Icon(Icons.settings, color: Color(0xFF00FFFF)),
              label: 'SETTINGS',
            ),
          ],
        ),
      ),
    );
  }
}

class ChatsTab extends StatefulWidget {
  const ChatsTab({super.key});

  @override
  State<ChatsTab> createState() => _ChatsTabState();
}

class _ChatsTabState extends State<ChatsTab> {
  List<dynamic> _contacts = [];
  bool _isLoading = true;
  final IntrovertClient _client = IntrovertClient();

  @override
  void initState() {
    super.initState();
    _loadContacts();
  }

  Future<void> _loadContacts() async {
    if (!mounted) return;
    setState(() => _isLoading = true);
    try {
      final contacts = _client.getContacts();
      if (mounted) {
        setState(() {
          _contacts = contacts;
          _isLoading = false;
        });
      }
    } catch (e) {
      debugPrint("Error loading contacts: $e");
      if (mounted) setState(() => _isLoading = false);
    }
  }

  void _showAddPeerDialog() {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _AddPeerDialog(
        onComplete: () {
          _loadContacts();
        },
      ),
    );
  }

  void _triggerManualSync() {
    _client.fetchMailbox();
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text("P2P Synchronization Triggered...")),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: _isLoading 
        ? const Center(child: CircularProgressIndicator(color: Colors.cyanAccent))
        : _contacts.isEmpty 
          ? Center(
              child: Padding(
                padding: const EdgeInsets.all(32.0),
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    const Icon(Icons.people_outline, size: 64, color: Colors.white10),
                    const SizedBox(height: 16),
                    const Text(
                      'No Sovereign Contacts Yet',
                      style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold, color: Colors.white38),
                    ),
                    const SizedBox(height: 8),
                    const Text(
                      'Invite a friend via Magic Wormhole to start an end-to-end encrypted session.',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: Colors.white24),
                    ),
                    const SizedBox(height: 24),
                    ElevatedButton.icon(
                      onPressed: _showAddPeerDialog,
                      icon: const Icon(Icons.person_add),
                      label: const Text("ADD CONTACT"),
                      style: ElevatedButton.styleFrom(
                        backgroundColor: Colors.white12,
                        foregroundColor: Colors.cyanAccent,
                      ),
                    ),
                  ],
                ),
              ),
            )
          : ListView.separated(
              itemCount: _contacts.length,
              separatorBuilder: (_, __) => const Divider(height: 1, indent: 80, color: Colors.white10),
              itemBuilder: (context, index) {
                final contact = _contacts[index];
                final peerId = contact['peer_id'] as String;
                return ListTile(
                  leading: CircleAvatar(
                    backgroundColor: Colors.cyanAccent.withValues(alpha: 0.1),
                    child: const Icon(Icons.person, color: Colors.cyanAccent),
                  ),
                  title: Text(
                    peerId, 
                    style: const TextStyle(fontFamily: 'monospace', fontSize: 13, fontWeight: FontWeight.bold),
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: Text(
                    contact['is_anchor_capable'] ? "ANCHOR CAPABLE" : "DIRECT PEER",
                    style: const TextStyle(fontSize: 10, color: Colors.white38),
                  ),
                  trailing: const Icon(Icons.chevron_right, color: Colors.white10),
                  onTap: () {
                    Navigator.push(
                      context,
                      MaterialPageRoute(builder: (context) => ChatScreen(peerId: peerId)),
                    );
                  },
                  onLongPress: () async {
                    final confirm = await showDialog<bool>(
                      context: context,
                      builder: (context) => AlertDialog(
                        backgroundColor: const Color(0xFF1A1F2B),
                        title: const Text("Delete Contact?", style: TextStyle(color: Colors.redAccent)),
                        content: Text("Remove $peerId from your verified contacts?"),
                        actions: [
                          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text("CANCEL")),
                          TextButton(
                            onPressed: () => Navigator.pop(context, true), 
                            child: const Text("DELETE", style: TextStyle(color: Colors.redAccent)),
                          ),
                        ],
                      ),
                    );

                    if (confirm == true) {
                      await _client.deleteContact(peerId);
                      _loadContacts();
                    }
                  },
                );
              },
            ),
      floatingActionButton: FloatingActionButton(
        onPressed: _showAddPeerDialog,
        backgroundColor: Colors.cyanAccent,
        foregroundColor: Colors.black,
        child: const Icon(Icons.person_add),
      ),
    );
  }
}

class _AddPeerDialog extends StatefulWidget {
  final VoidCallback onComplete;
  const _AddPeerDialog({required this.onComplete});

  @override
  State<_AddPeerDialog> createState() => _AddPeerDialogState();
}

class _AddPeerDialogState extends State<_AddPeerDialog> {
  final IntrovertClient _client = IntrovertClient();
  final TextEditingController _codeController = TextEditingController();
  StreamSubscription<NetworkEvent>? _networkSubscription;
  
  String? _inviteCode;
  bool _isWaiting = false;
  String _status = "Select an onboarding method";

  @override
  void initState() {
    super.initState();
    _networkSubscription = _client.networkStream.listen((event) {
      if (!mounted) return;
      if (event.type == 6) {
        // Event 6: Code Generated
        setState(() {
          _inviteCode = String.fromCharCodes(event.data);
          _status = "Share this code with your peer:";
          _isWaiting = false;
        });
      } else if (event.type == 7) {
        // Event 7: Handover Complete
        widget.onComplete();
        Navigator.pop(context);
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text("Contact Verified Successfully!")),
        );
      }
    });
  }

  @override
  void dispose() {
    _networkSubscription?.cancel();
    _codeController.dispose();
    super.dispose();
  }

  void _startInvite() {
    setState(() {
      _isWaiting = true;
      _status = "Generating Magic Wormhole code...";
    });
    _client.startWormholeInvite();
  }

  void _joinInvite() {
    final code = _codeController.text.trim();
    if (code.isEmpty) return;

    setState(() {
      _isWaiting = true;
      _status = "Joining session...";
    });
    _client.joinWormholeInvite(code);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: const Color(0xFF1A1F2B),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      title: const Text("Add Sovereign Peer", style: TextStyle(color: Colors.cyanAccent)),
      content: SizedBox(
        width: double.maxFinite,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(_status, style: const TextStyle(color: Colors.white70, fontSize: 14)),
            const SizedBox(height: 20),
            if (_inviteCode != null) ...[
              Container(
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: Colors.black26,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: Colors.cyanAccent.withValues(alpha: 0.3)),
                ),
                child: SelectableText(
                  _inviteCode!,
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    fontSize: 20, 
                    fontWeight: FontWeight.bold, 
                    color: Colors.cyanAccent,
                    letterSpacing: 2,
                  ),
                ),
              ),
              const SizedBox(height: 12),
              const Text("Waiting for peer to join...", style: TextStyle(fontSize: 12, color: Colors.white38)),
            ] else if (!_isWaiting) ...[
              ElevatedButton(
                onPressed: _startInvite,
                style: ElevatedButton.styleFrom(
                  minimumSize: const Size(double.infinity, 50),
                  backgroundColor: Colors.cyanAccent,
                  foregroundColor: Colors.black,
                ),
                child: const Text("CREATE INVITE CODE"),
              ),
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 16.0),
                child: Text("OR", style: TextStyle(color: Colors.white24)),
              ),
              TextField(
                controller: _codeController,
                decoration: InputDecoration(
                  hintText: "ENTER PEER'S CODE",
                  hintStyle: const TextStyle(color: Colors.white24),
                  filled: true,
                  fillColor: Colors.black26,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
                ),
                style: const TextStyle(color: Colors.white, fontFamily: 'monospace'),
              ),
              const SizedBox(height: 12),
              ElevatedButton(
                onPressed: _joinInvite,
                style: ElevatedButton.styleFrom(
                  minimumSize: const Size(double.infinity, 50),
                  backgroundColor: Colors.white12,
                  foregroundColor: Colors.white,
                ),
                child: const Text("JOIN SESSION"),
              ),
            ] else
              const CircularProgressIndicator(color: Colors.cyanAccent),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text("CANCEL", style: TextStyle(color: Colors.white38)),
        ),
      ],
    );
  }
}

class CallsTab extends StatefulWidget {
  const CallsTab({super.key});

  @override
  State<CallsTab> createState() => _CallsTabState();
}

class _CallsTabState extends State<CallsTab> {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Calls', style: TextStyle(fontWeight: FontWeight.bold)),
      ),
      body: const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.call_end, size: 64, color: Colors.white24),
            SizedBox(height: 16),
            Text('VoIP re-implementation in progress.', textAlign: TextAlign.center),
          ],
        ),
      ),
    );
  }
}

class SettingsTab extends StatefulWidget {
  const SettingsTab({super.key});

  @override
  State<SettingsTab> createState() => _SettingsTabState();
}

class _SettingsTabState extends State<SettingsTab> {
  Map<String, dynamic> _economyStats = {
    'sol_balance': 0,
    'pending_rewards': 0,
    'total_relayed': 0,
    'sol_address': 'Connecting...',
  };

  @override
  void initState() {
    super.initState();
    _startMonitoring();
  }

  void _startMonitoring() {
    IntrovertClient().startEconomyMonitoring((stats) {
      if (mounted) {
        setState(() {
          _economyStats = stats;
        });
      }
    });
  }

  Future<void> _handleClaim() async {
    try {
      final sig = await IntrovertClient().claimRewards();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Rewards claimed! TX: ${sig.substring(0, 10)}...')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Claim failed: ${e.toString()}')),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final client = IntrovertClient();
    final localPeerId = client.getPeerId() ?? "ERROR";

    return Scaffold(
      appBar: AppBar(title: const Text('Settings', style: TextStyle(fontWeight: FontWeight.bold))),
      body: SingleChildScrollView(
        child: Column(
          children: [
            const SizedBox(height: 20),
            Center(
              child: Column(
                children: [
                  Container(
                    width: 100,
                    height: 100,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(color: const Color(0xFF00FFFF), width: 2),
                      gradient: const LinearGradient(
                        colors: [Color(0xFF0A0E17), Color(0xFF1A1F2B)],
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                      ),
                    ),
                    child: const Icon(Icons.security, size: 50, color: Color(0xFF00FFFF)),
                  ),
                  const SizedBox(height: 16),
                  const Text('Identity Hub', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                ],
              ),
            ),
            const SizedBox(height: 32),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16.0),
              child: SovereignEarnings(
                economyStats: _economyStats,
                onClaim: _handleClaim,
              ),
            ),
            const SizedBox(height: 24),
            _buildSettingSection(
              'Network Identity',
              [
                ListTile(
                  title: const Text('Public Peer ID'),
                  subtitle: Text(localPeerId, 
                    style: const TextStyle(fontFamily: 'monospace', fontSize: 12)),
                  trailing: IconButton(
                    icon: const Icon(Icons.copy),
                    onPressed: () {
                      Clipboard.setData(ClipboardData(text: localPeerId));
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Peer ID copied to clipboard')),
                      );
                    },
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            _buildSettingSection(
              'Node Status',
              [
                const ListTile(
                  leading: Icon(Icons.check_circle, color: Colors.green),
                  title: Text('Bulletproof Core Active'),
                ),
                const ListTile(
                  leading: Icon(Icons.storage),
                  title: Text('SQLCipher Storage Encrypted'),
                  trailing: Icon(Icons.check_circle, color: Colors.green),
                ),
                ListTile(
                  leading: const Icon(Icons.delete_sweep, color: Colors.redAccent),
                  title: const Text('Clear All Contacts', style: TextStyle(color: Colors.redAccent)),
                  onTap: () async {
                    final confirm = await showDialog<bool>(
                      context: context,
                      builder: (context) => AlertDialog(
                        backgroundColor: const Color(0xFF1A1F2B),
                        title: const Text("Destructive Action", style: TextStyle(color: Colors.redAccent)),
                        content: const Text("Permanently delete all verified contacts and cached sessions?"),
                        actions: [
                          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text("CANCEL")),
                          TextButton(
                            onPressed: () => Navigator.pop(context, true), 
                            child: const Text("CLEAR EVERYTHING", style: TextStyle(color: Colors.redAccent)),
                          ),
                        ],
                      ),
                    );

                    if (confirm == true) {
                      await IntrovertClient().clearAllContacts();
                      if (mounted) {
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(content: Text("All contacts cleared.")),
                        );
                      }
                    }
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSettingSection(String title, List<Widget> children) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          child: Text(
            title.toUpperCase(),
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.bold, color: Colors.grey),
          ),
        ),
        ...children,
      ],
    );
  }
}
