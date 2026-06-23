import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

class ThemeConfig {
  final String name;
  final Color bg;
  final Color surface;
  final Color text;
  final Color mutedText;
  final Color accent;
  final bool isDark;
  final String? wallpaperPath;
  final double wallpaperOpacity;

  const ThemeConfig({
    required this.name,
    required this.bg,
    required this.surface,
    required this.text,
    required this.mutedText,
    required this.accent,
    this.isDark = true,
    this.wallpaperPath,
    this.wallpaperOpacity = 0.3,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'bg': bg.value,
    'surface': surface.value,
    'text': text.value,
    'mutedText': mutedText.value,
    'accent': accent.value,
    'isDark': isDark,
    'wallpaperPath': wallpaperPath,
    'wallpaperOpacity': wallpaperOpacity,
  };

  factory ThemeConfig.fromJson(Map<String, dynamic> json) => ThemeConfig(
    name: json['name'] as String,
    bg: Color(json['bg'] as int),
    surface: Color(json['surface'] as int),
    text: Color(json['text'] as int),
    mutedText: Color(json['mutedText'] as int),
    accent: Color(json['accent'] as int),
    isDark: json['isDark'] as bool? ?? true,
    wallpaperPath: json['wallpaperPath'] as String?,
    wallpaperOpacity: (json['wallpaperOpacity'] as num?)?.toDouble() ?? 0.3,
  );
}

class AppTheme extends ChangeNotifier {
  static final AppTheme _instance = AppTheme._internal();
  static AppTheme get current => _instance;

  AppTheme._internal();

  static const ThemeConfig introvertDefault = ThemeConfig(
    name: "Introvert Dark",
    bg: Color(0xFF0A0E17),
    surface: Color(0xFF1A1F2B),
    text: Colors.white,
    mutedText: Colors.white54,
    accent: Color(0xFF1AFFFF),
  );

  static const List<ThemeConfig> themes = [
    introvertDefault,
    ThemeConfig(
      name: "Amber Hollow",
      bg: Color(0xFF160F0A),
      surface: Color(0xFF271B12),
      text: Color(0xFFFFF1E0),
      mutedText: Color(0xFFB89A7A),
      accent: Color(0xFF1AFFFF),
    ),
    ThemeConfig(
      name: "Azure Sky",
      bg: Color(0xFFF0F6FF),
      surface: Color(0xFFFFFFFF),
      text: Color(0xFF0A1A33),
      mutedText: Color(0xFF5580AA),
      accent: Color(0xFF0077CC),
      isDark: false,
      wallpaperPath: 'assets/images/theme_light3.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Beach House",
      bg: Color(0xFFFAF7F2),
      surface: Color(0xFFFFFFFF),
      text: Color(0xFF2B2620),
      mutedText: Color(0xFF8C8275),
      accent: Color(0xFF009494),
      isDark: false,
      wallpaperPath: 'assets/images/theme_beach.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Canyon",
      bg: Color(0xFF1A0F0A),
      surface: Color(0xFF2B1A10),
      text: Color(0xFFFFF0E0),
      mutedText: Color(0xFFBF8A60),
      accent: Color(0xFFFF6B35),
      wallpaperPath: 'assets/images/theme_canyon.jpg',
      wallpaperOpacity: 1.0,
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
      name: "Cyber City",
      bg: Color(0xFF0A0E17),
      surface: Color(0xFF121825),
      text: Color(0xFFE0E6F0),
      mutedText: Color(0xFF6B7A99),
      accent: Color(0xFF00E5FF),
      wallpaperPath: 'assets/images/theme_cybercity.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Cyber City II",
      bg: Color(0xFF0D0815),
      surface: Color(0xFF1A1028),
      text: Color(0xFFF0E0FF),
      mutedText: Color(0xFF9070B0),
      accent: Color(0xFFE040FB),
      wallpaperPath: 'assets/images/theme_cyber_city2.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Cyber City III",
      bg: Color(0xFF080D0A),
      surface: Color(0xFF102018),
      text: Color(0xFFE0FFE8),
      mutedText: Color(0xFF70B080),
      accent: Color(0xFF00FF60),
      wallpaperPath: 'assets/images/theme_cyber_city3.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Desert",
      bg: Color(0xFF1A1508),
      surface: Color(0xFF2B2412),
      text: Color(0xFFFFF5E0),
      mutedText: Color(0xFFBFAB70),
      accent: Color(0xFFFFB830),
      wallpaperPath: 'assets/images/theme_desert.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Forest",
      bg: Color(0xFF0A120A),
      surface: Color(0xFF142014),
      text: Color(0xFFE0F0E0),
      mutedText: Color(0xFF6B9A6B),
      accent: Color(0xFF34D399),
      wallpaperPath: 'assets/images/theme_forest.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Golden Hour",
      bg: Color(0xFFFFF8F0),
      surface: Color(0xFFFFFFFF),
      text: Color(0xFF2B1A0A),
      mutedText: Color(0xFF997A55),
      accent: Color(0xFFE67E22),
      isDark: false,
      wallpaperPath: 'assets/images/theme_light2.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Morning Dew",
      bg: Color(0xFFF5F8FA),
      surface: Color(0xFFFFFFFF),
      text: Color(0xFF1A2332),
      mutedText: Color(0xFF6B8299),
      accent: Color(0xFF2196F3),
      isDark: false,
      wallpaperPath: 'assets/images/theme_light1.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Mountain Peak",
      bg: Color(0xFF0D1117),
      surface: Color(0xFF161B22),
      text: Color(0xFFE6EDF3),
      mutedText: Color(0xFF7D8590),
      accent: Color(0xFF58A6FF),
      wallpaperPath: 'assets/images/theme_mountain1.jpg',
      wallpaperOpacity: 1.0,
    ),
    ThemeConfig(
      name: "Mountain Ridge",
      bg: Color(0xFF0E1015),
      surface: Color(0xFF1A1F2B),
      text: Color(0xFFE8EAED),
      mutedText: Color(0xFF8A9099),
      accent: Color(0xFF8B5CF6),
      wallpaperPath: 'assets/images/theme_mountain2.jpg',
      wallpaperOpacity: 1.0,
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
      name: "Winter Wonderland",
      bg: Color(0xFF0E1520),
      surface: Color(0xFF182030),
      text: Color(0xFFE8F0FF),
      mutedText: Color(0xFF7A99C0),
      accent: Color(0xFF60CFFF),
      wallpaperPath: 'assets/images/theme_winter.jpg',
      wallpaperOpacity: 1.0,
    ),
  ];

  ThemeConfig _currentTheme = introvertDefault;
  List<ThemeConfig> _customThemes = [];

  ThemeConfig get theme => _currentTheme;
  List<ThemeConfig> get customThemes => List.unmodifiable(_customThemes);

  Color get bg => _currentTheme.bg;
  Color get surface => _currentTheme.surface;
  Color get text => _currentTheme.text;
  Color get mutedText => _currentTheme.mutedText;
  Color get accent => _currentTheme.accent;
  bool get isDark => _currentTheme.isDark;
  String? get wallpaperPath => _currentTheme.wallpaperPath;
  double get wallpaperOpacity => _currentTheme.wallpaperOpacity;

  List<ThemeConfig> get allThemes => [...themes, ..._customThemes];

  Future<void> loadTheme() async {
    final prefs = await SharedPreferences.getInstance();
    
    // Load custom themes
    final customThemesJson = prefs.getString('custom_themes') ?? '[]';
    try {
      final List<dynamic> list = json.decode(customThemesJson);
      _customThemes = list.map((e) => ThemeConfig.fromJson(e as Map<String, dynamic>)).toList();
    } catch (_) {
      _customThemes = [];
    }

    // Load current theme name
    final themeName = prefs.getString('app_theme') ?? "Introvert Dark";
    _currentTheme = allThemes.firstWhere((t) => t.name == themeName, orElse: () => introvertDefault);
    notifyListeners();
  }

  Future<void> setTheme(ThemeConfig newTheme) async {
    _currentTheme = newTheme;
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('app_theme', newTheme.name);
  }

  Future<void> saveCustomTheme(ThemeConfig theme) async {
    // Remove existing theme with same name
    _customThemes.removeWhere((t) => t.name == theme.name);
    _customThemes.add(theme);
    await _saveCustomThemes();
    notifyListeners();
  }

  Future<void> deleteCustomTheme(String name) async {
    _customThemes.removeWhere((t) => t.name == name);
    await _saveCustomThemes();
    // If the deleted theme was active, fall back to default
    if (_currentTheme.name == name) {
      await setTheme(introvertDefault);
    } else {
      notifyListeners();
    }
  }

  Future<void> _saveCustomThemes() async {
    final prefs = await SharedPreferences.getInstance();
    final jsonList = _customThemes.map((t) => t.toJson()).toList();
    await prefs.setString('custom_themes', json.encode(jsonList));
  }
}
