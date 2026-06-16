import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

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

  ThemeConfig _currentTheme = introvertDefault;

  ThemeConfig get theme => _currentTheme;
  
  Color get bg => _currentTheme.bg;
  Color get surface => _currentTheme.surface;
  Color get text => _currentTheme.text;
  Color get mutedText => _currentTheme.mutedText;
  Color get accent => _currentTheme.accent;

  Future<void> loadTheme() async {
    final prefs = await SharedPreferences.getInstance();
    final themeName = prefs.getString('app_theme') ?? "Introvert Dark";
    _currentTheme = themes.firstWhere((t) => t.name == themeName, orElse: () => introvertDefault);
    notifyListeners();
  }

  Future<void> setTheme(ThemeConfig newTheme) async {
    _currentTheme = newTheme;
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('app_theme', newTheme.name);
  }
}
