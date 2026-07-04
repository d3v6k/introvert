import 'package:flutter/material.dart';

class ThemeConfig {
  final String name;
  final Color bg;
  final Color surface;
  final Color text;
  final Color mutedText;
  final Color accent;

  const ThemeConfig({
    required this.name,
    required this.bg,
    required this.surface,
    required this.text,
    required this.mutedText,
    required this.accent,
  });
}

const List<ThemeConfig> mockupThemes = [
  ThemeConfig(
    name: "Introvert Dark",
    bg: Color(0xFF0A0E17),
    surface: Color(0xFF1A1F2B),
    text: Color(0xFFFFFFFF),
    mutedText: Color(0x8AFFFFFF),
    accent: Color(0xFF1AFFFF),
  ),
  ThemeConfig(
    name: "Velvet Plum",
    bg: Color(0xFF150A1A),
    surface: Color(0xFF241430),
    text: Color(0xFFF3E8FF),
    mutedText: Color(0xFF9D8AAE),
    accent: Color(0xFF1AFFFF),
  ),
  ThemeConfig(
    name: "Carbon Slate",
    bg: Color(0xFF0E1113),
    surface: Color(0xFF1C2125),
    text: Color(0xFFE8EAED),
    mutedText: Color(0xFF8A9099),
    accent: Color(0xFF1AFFFF),
  ),
  ThemeConfig(
    name: "Amber Hollow",
    bg: Color(0xFF160F0A),
    surface: Color(0xFF271B12),
    text: Color(0xFFFFF1E0),
    mutedText: Color(0xFFB89A7A),
    accent: Color(0xFF1AFFFF),
  ),
  ThemeConfig(
    name: "Linen Mist",
    bg: Color(0xFFFAF7F2),
    surface: Color(0xFFFFFFFF),
    text: Color(0xFF2B2620),
    mutedText: Color(0xFF8C8275),
    accent: Color(0xFF009494),
  ),
  ThemeConfig(
    name: "Glacier Bloom",
    bg: Color(0xFFEEF6F4),
    surface: Color(0xFFFFFFFF),
    text: Color(0xFF16332E),
    mutedText: Color(0xFF6B9A91),
    accent: Color(0xFF009494),
  ),
  ThemeConfig(
    name: "Rose Quartz",
    bg: Color(0xFFFBF1F2),
    surface: Color(0xFFFFFFFF),
    text: Color(0xFF3A2228),
    mutedText: Color(0xFFA8818A),
    accent: Color(0xFF009494),
  ),
];

class ThemeMockupGridScreen extends StatelessWidget {
  const ThemeMockupGridScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black,
      appBar: AppBar(
        title: const Text('Theme Mockups', style: TextStyle(color: Colors.white, fontWeight: FontWeight.w400, fontSize: 16)),
        backgroundColor: Colors.black,
        iconTheme: const IconThemeData(color: Colors.white),
      ),
      body: LayoutBuilder(
        builder: (context, constraints) {
          int columns = constraints.maxWidth > 800 ? 3 : 2;
          if (constraints.maxWidth < 500) columns = 1;

          return GridView.builder(
            padding: const EdgeInsets.all(20),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: columns,
              childAspectRatio: 0.55,
              crossAxisSpacing: 20,
              mainAxisSpacing: 20,
            ),
            itemCount: mockupThemes.length,
            itemBuilder: (context, index) {
              return Material(
                color: Colors.transparent,
                child: MockupAppShell(theme: mockupThemes[index]),
              );
            },
          );
        },
      ),
    );
  }
}

class MockupAppShell extends StatelessWidget {
  final ThemeConfig theme;

  const MockupAppShell({super.key, required this.theme});

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: theme.bg,
        borderRadius: BorderRadius.circular(28),
        boxShadow: [
          BoxShadow(color: Colors.black.withValues(alpha: 0.25), blurRadius: 20, offset: const Offset(0, 8)),
        ],
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          // App Bar
          Container(
            color: theme.surface,
            padding: const EdgeInsets.only(top: 28, bottom: 14, left: 20, right: 20),
            child: Row(
              children: [
                Image.asset(
                  theme.bg.computeLuminance() > 0.5
                      ? 'assets/images/logo_black.png'
                      : 'assets/images/logo_white.png',
                  height: 16,
                  fit: BoxFit.contain,
                  filterQuality: FilterQuality.high,
                  errorBuilder: (context, error, stackTrace) => Image.asset('assets/images/logo.png', height: 16),
                ),
                const SizedBox(width: 8),
                Container(
                  width: 5,
                  height: 5,
                  decoration: const BoxDecoration(shape: BoxShape.circle, color: Colors.greenAccent),
                ),
                const SizedBox(width: 6),
                Text("online", style: TextStyle(color: theme.mutedText, fontSize: 9, fontWeight: FontWeight.w400)),
                const Spacer(),
                Icon(Icons.speed_rounded, color: theme.accent, size: 18),
                const SizedBox(width: 20),
                Icon(Icons.account_circle_outlined, color: theme.mutedText, size: 18),
              ],
            ),
          ),

          // Theme Name Banner
          Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 5),
            color: theme.accent.withValues(alpha: 0.1),
            child: Text(
              theme.name.toUpperCase(),
              textAlign: TextAlign.center,
              style: TextStyle(
                color: theme.accent,
                fontSize: 9,
                fontWeight: FontWeight.w500,
                letterSpacing: 2,
              ),
            ),
          ),

          // Chat List
          Expanded(
            child: ListView(
              padding: const EdgeInsets.only(top: 8, bottom: 0),
              physics: const NeverScrollableScrollPhysics(),
              children: [
                _buildMockContact("Sovereign Group", "Encrypted Mesh Room", true, theme),
                _buildMockContact("Alice", "DIRECT PEER", false, theme),
                _buildMockContact("Bob", "ANCHOR CAPABLE", false, theme),
                _buildMockContact("Charlie", "DIRECT PEER", false, theme),
              ],
            ),
          ),

          // FAB
          Align(
            alignment: Alignment.centerRight,
            child: Padding(
              padding: const EdgeInsets.only(right: 16, bottom: 4),
              child: Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: theme.accent,
                  shape: BoxShape.circle,
                  boxShadow: [
                    BoxShadow(color: theme.accent.withValues(alpha: 0.2), blurRadius: 8, offset: const Offset(0, 3)),
                  ],
                ),
                child: const Icon(Icons.add, color: Colors.black, size: 20),
              ),
            ),
          ),

          // Floating Pill Nav Bar
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 0, 12, 10),
            child: Container(
              height: 52,
              decoration: BoxDecoration(
                color: theme.surface.withValues(alpha: 0.9),
                borderRadius: BorderRadius.circular(26),
                boxShadow: [
                  BoxShadow(color: Colors.black.withValues(alpha: 0.15), blurRadius: 12, offset: const Offset(0, 4)),
                ],
              ),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                children: [
                  _buildMockTab(0, Icons.chat_bubble_outline_rounded, Icons.chat_bubble_rounded, "CHATS", theme),
                  _buildMockTab(1, Icons.cloud_queue_rounded, Icons.cloud_rounded, "DRIVE", theme),
                  _buildMockTab(2, Icons.settings_outlined, Icons.settings_rounded, "SETTINGS", theme),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildMockContact(String title, String subtitle, bool isGroup, ThemeConfig theme) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 14),
      child: Row(
        children: [
          CircleAvatar(
            backgroundColor: theme.accent.withValues(alpha: 0.08),
            radius: 22,
            child: Icon(isGroup ? Icons.group_rounded : Icons.person_rounded, color: theme.accent, size: 18),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        title,
                        style: TextStyle(color: theme.text, fontSize: 13, fontWeight: FontWeight.w500),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    if (isGroup) ...[
                      const SizedBox(width: 8),
                      Text("GROUP", style: TextStyle(color: theme.accent, fontSize: 8, fontWeight: FontWeight.w500, letterSpacing: 0.5)),
                    ]
                  ],
                ),
                const SizedBox(height: 6),
                Text(
                  subtitle,
                  style: TextStyle(color: theme.mutedText, fontSize: 10, fontWeight: FontWeight.w400),
                ),
              ],
            ),
          ),
          Icon(Icons.videocam_rounded, color: theme.mutedText, size: 16),
          const SizedBox(width: 16),
          Icon(Icons.more_vert, color: theme.mutedText, size: 16),
        ],
      ),
    );
  }

  Widget _buildMockTab(int index, IconData outlineIcon, IconData filledIcon, String label, ThemeConfig theme) {
    final isSelected = index == 0;
    return SizedBox(
      width: 64,
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          AnimatedContainer(
            duration: const Duration(milliseconds: 200),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
            decoration: isSelected
                ? BoxDecoration(
                    color: theme.accent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(14),
                  )
                : null,
            child: Icon(
              isSelected ? filledIcon : outlineIcon,
              color: isSelected ? theme.accent : theme.mutedText,
              size: 20,
            ),
          ),
          const SizedBox(height: 2),
          Text(
            label,
            style: TextStyle(
              color: isSelected ? theme.accent : theme.mutedText,
              fontSize: 9,
              fontWeight: isSelected ? FontWeight.w500 : FontWeight.w400,
              letterSpacing: 0.3,
            ),
          ),
        ],
      ),
    );
  }
}
