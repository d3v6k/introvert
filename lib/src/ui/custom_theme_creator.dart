import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:image/image.dart' as img;
import 'package:image_picker/image_picker.dart';
import 'package:path_provider/path_provider.dart';
import '../../theme/app_theme.dart';

class CustomThemeCreator extends StatefulWidget {
  final ThemeConfig? existingTheme;

  const CustomThemeCreator({this.existingTheme, super.key});

  @override
  State<CustomThemeCreator> createState() => _CustomThemeCreatorState();
}

class _CustomThemeCreatorState extends State<CustomThemeCreator> {
  late TextEditingController _nameController;
  late bool _isDark;
  late Color _bg;
  late Color _surface;
  late Color _text;
  late Color _mutedText;
  late Color _accent;
  String? _wallpaperPath;
  double _wallpaperOpacity = 0.3;
  String? _nameError;

  String _generateDefaultName() {
    final existing = AppTheme.current.customThemes.map((t) => t.name).toSet();
    existing.addAll(AppTheme.themes.map((t) => t.name));
    for (int i = 1; i <= 99; i++) {
      final name = 'custom${i.toString().padLeft(2, '0')}';
      if (!existing.contains(name)) return name;
    }
    return 'custom01';
  }

  @override
  void initState() {
    super.initState();
    final theme = widget.existingTheme;
    if (theme != null) {
      // Check if this is a default theme (not custom) — auto-generate custom name
      final isDefault = AppTheme.themes.any((t) => t.name == theme.name);
      if (isDefault) {
        _nameController = TextEditingController(text: _generateDefaultName());
      } else {
        _nameController = TextEditingController(text: theme.name);
      }
    } else {
      _nameController = TextEditingController(text: _generateDefaultName());
    }
    _isDark = theme?.isDark ?? true;
    _bg = theme?.bg ?? const Color(0xFF0A0E17);
    _surface = theme?.surface ?? const Color(0xFF1A1F2B);
    _text = theme?.text ?? Colors.white;
    _mutedText = theme?.mutedText ?? Colors.white54;
    _accent = theme?.accent ?? const Color(0xFF1AFFFF);
    _wallpaperPath = theme?.wallpaperPath;
    _wallpaperOpacity = theme?.wallpaperOpacity ?? 0.3;
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _pickColor(Color current, ValueChanged<Color> onPicked) {
    showDialog(
      context: context,
      builder: (context) => _ColorPickerDialog(
        currentColor: current,
        onColorSelected: (color) {
          onPicked(color);
          Navigator.pop(context);
        },
      ),
    );
  }

  Future<void> _pickWallpaper() async {
    final picker = ImagePicker();
    final picked = await picker.pickImage(source: ImageSource.gallery);
    if (picked == null) return;
    
    final bytes = await picked.readAsBytes();
    final original = img.decodeImage(bytes);
    if (original == null) return;
    
    const targetWidth = 720;
    final resized = img.copyResize(original, width: targetWidth, interpolation: img.Interpolation.linear);
    final resizedBytes = img.encodeJpg(resized, quality: 80);
    
    final appDir = await getApplicationDocumentsDirectory();
    final wallpaperDir = Directory('${appDir.path}/wallpapers');
    if (!await wallpaperDir.exists()) await wallpaperDir.create(recursive: true);
    
    final destPath = '${wallpaperDir.path}/wallpaper_${DateTime.now().millisecondsSinceEpoch}.jpg';
    await File(destPath).writeAsBytes(resizedBytes);
    
    setState(() => _wallpaperPath = destPath);
  }

  Widget _buildColorRow(String label, Color color, ValueChanged<Color> onPicked) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      title: Text(label, style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
      trailing: GestureDetector(
        onTap: () => _pickColor(color, onPicked),
        child: Container(
          width: 36,
          height: 36,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(
              color: AppTheme.current.mutedText.withValues(alpha: 0.3),
              width: 1.5,
            ),
          ),
        ),
      ),
      onTap: () => _pickColor(color, onPicked),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: 400),
        child: SingleChildScrollView(
          padding: EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                widget.existingTheme != null ? "EDIT THEME" : "CREATE THEME",
                style: TextStyle(
                  color: AppTheme.current.accent,
                  fontSize: 12,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 1.2,
                ),
              ),
              SizedBox(height: 16),

              // Theme name
              TextField(
                controller: _nameController,
                style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                decoration: InputDecoration(
                  hintText: "Theme name",
                  hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                  errorText: _nameError,
                  errorStyle: TextStyle(color: Colors.redAccent, fontSize: 12),
                  filled: true,
                  fillColor: AppTheme.current.text.withValues(alpha: 0.05),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                    borderSide: BorderSide.none,
                  ),
                  errorBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                    borderSide: BorderSide(color: Colors.redAccent, width: 1.5),
                  ),
                  focusedErrorBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                    borderSide: BorderSide(color: Colors.redAccent, width: 1.5),
                  ),
                  contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 12),
                ),
                onChanged: (_) {
                  if (_nameError != null) setState(() => _nameError = null);
                },
              ),
              SizedBox(height: 16),

              // Light/Dark toggle
              Row(
                children: [
                  Text("Theme mode:", style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                  SizedBox(width: 12),
                  ChoiceChip(
                    label: Text("Dark", style: TextStyle(fontSize: 12)),
                    selected: _isDark,
                    selectedColor: AppTheme.current.accent.withValues(alpha: 0.2),
                    labelStyle: TextStyle(
                      color: _isDark ? AppTheme.current.accent : AppTheme.current.mutedText,
                      fontWeight: _isDark ? FontWeight.bold : FontWeight.normal,
                    ),
                    onSelected: (_) => setState(() => _isDark = true),
                  ),
                  SizedBox(width: 8),
                  ChoiceChip(
                    label: Text("Light", style: TextStyle(fontSize: 12)),
                    selected: !_isDark,
                    selectedColor: AppTheme.current.accent.withValues(alpha: 0.2),
                    labelStyle: TextStyle(
                      color: !_isDark ? AppTheme.current.accent : AppTheme.current.mutedText,
                      fontWeight: !_isDark ? FontWeight.bold : FontWeight.normal,
                    ),
                    onSelected: (_) => setState(() => _isDark = false),
                  ),
                ],
              ),
              SizedBox(height: 16),

              // Live preview
              Container(
                padding: EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: _bg,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: _mutedText.withValues(alpha: 0.2)),
                  image: _wallpaperPath != null && File(_wallpaperPath!).existsSync()
                      ? DecorationImage(
                          image: FileImage(File(_wallpaperPath!)),
                          fit: BoxFit.cover,
                          opacity: _wallpaperOpacity,
                        )
                      : null,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text("Preview", style: TextStyle(color: _accent, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 1)),
                    SizedBox(height: 8),
                    Container(
                      padding: EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: _surface,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(_nameController.text.isEmpty ? "Theme Name" : _nameController.text,
                            style: TextStyle(color: _text, fontSize: 16, fontWeight: FontWeight.bold)),
                          SizedBox(height: 4),
                          Text("Message preview text", style: TextStyle(color: _mutedText, fontSize: 12)),
                          SizedBox(height: 8),
                          Container(
                            padding: EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                            decoration: BoxDecoration(color: _accent, borderRadius: BorderRadius.circular(16)),
                            child: Text("Accent Button", style: TextStyle(color: _isDark ? Colors.black : Colors.white, fontSize: 11, fontWeight: FontWeight.bold)),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              SizedBox(height: 20),

              // Color pickers
              Text("COLOURS", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 1)),
              SizedBox(height: 8),
              _buildColorRow("Background", _bg, (c) => setState(() => _bg = c)),
              _buildColorRow("Surface", _surface, (c) => setState(() => _surface = c)),
              _buildColorRow("Text", _text, (c) => setState(() => _text = c)),
              _buildColorRow("Muted Text", _mutedText, (c) => setState(() => _mutedText = c)),
              _buildColorRow("Accent", _accent, (c) => setState(() => _accent = c)),
              SizedBox(height: 20),

              // Wallpaper
              Text("WALLPAPER", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 1)),
              SizedBox(height: 8),
              Row(
                children: [
                  Expanded(
                    child: GestureDetector(
                      onTap: _pickWallpaper,
                      child: Container(
                        height: 80,
                        decoration: BoxDecoration(
                          color: AppTheme.current.text.withValues(alpha: 0.05),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.2)),
                          image: _wallpaperPath != null && File(_wallpaperPath!).existsSync()
                              ? DecorationImage(image: FileImage(File(_wallpaperPath!)), fit: BoxFit.cover)
                              : null,
                        ),
                        child: _wallpaperPath == null || !File(_wallpaperPath!).existsSync()
                            ? Column(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  Icon(Icons.wallpaper, color: AppTheme.current.mutedText.withValues(alpha: 0.5), size: 28),
                                  SizedBox(height: 4),
                                  Text("Tap to set wallpaper", style: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5), fontSize: 11)),
                                ],
                              )
                            : null,
                      ),
                    ),
                  ),
                  if (_wallpaperPath != null) ...[
                    SizedBox(width: 8),
                    GestureDetector(
                      onTap: () => setState(() => _wallpaperPath = null),
                      child: Icon(Icons.close, color: AppTheme.current.mutedText, size: 20),
                    ),
                  ],
                ],
              ),
              if (_wallpaperPath != null) ...[
                SizedBox(height: 12),
                Row(
                  children: [
                    Text("Opacity", style: TextStyle(color: AppTheme.current.text, fontSize: 14)),
                    SizedBox(width: 8),
                    Expanded(
                      child: SliderTheme(
                        data: SliderTheme.of(context).copyWith(
                          activeTrackColor: AppTheme.current.accent,
                          inactiveTrackColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                          thumbColor: AppTheme.current.accent,
                          overlayColor: AppTheme.current.accent.withValues(alpha: 0.2),
                        ),
                        child: Slider(
                          value: _wallpaperOpacity,
                          min: 0.0,
                          max: 1.0,
                          onChanged: (v) => setState(() => _wallpaperOpacity = v),
                        ),
                      ),
                    ),
                    Text("${(_wallpaperOpacity * 100).toInt()}%", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12)),
                  ],
                ),
              ],
              SizedBox(height: 20),

              // Actions
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: () => Navigator.pop(context),
                    child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText)),
                  ),
                  SizedBox(width: 8),
                  ElevatedButton(
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.current.accent,
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                    ),
                    onPressed: () {
                      final name = _nameController.text.trim();
                      if (name.isEmpty) {
                        setState(() => _nameError = "Please enter a theme name");
                        return;
                      }
                      final theme = ThemeConfig(
                        name: name,
                        bg: _bg,
                        surface: _surface,
                        text: _text,
                        mutedText: _mutedText,
                        accent: _accent,
                        isDark: _isDark,
                        wallpaperPath: _wallpaperPath,
                        wallpaperOpacity: _wallpaperOpacity,
                      );
                      Navigator.pop(context, theme);
                    },
                    child: Text("SAVE", style: TextStyle(color: _isDark ? Colors.black : Colors.white, fontWeight: FontWeight.bold)),
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

class _ColorPickerDialog extends StatefulWidget {
  final Color currentColor;
  final ValueChanged<Color> onColorSelected;

  const _ColorPickerDialog({required this.currentColor, required this.onColorSelected});

  @override
  State<_ColorPickerDialog> createState() => _ColorPickerDialogState();
}

class _ColorPickerDialogState extends State<_ColorPickerDialog> {
  late double _hue;
  late double _saturation;
  late double _value;
  late TextEditingController _hexController;

  @override
  void initState() {
    super.initState();
    final hsv = HSVColor.fromColor(widget.currentColor);
    _hue = hsv.hue;
    _saturation = hsv.saturation;
    _value = hsv.value;
    _hexController = TextEditingController(text: _colorToHex(widget.currentColor));
  }

  @override
  void dispose() {
    _hexController.dispose();
    super.dispose();
  }

  String _colorToHex(Color c) => '#${c.value.toRadixString(16).substring(2).toUpperCase()}';

  Color get _currentColor => HSVColor.fromAHSV(1, _hue, _saturation, _value).toColor();

  void _updateFromHex(String hex) {
    final cleaned = hex.replaceAll('#', '');
    if (cleaned.length == 6) {
      try {
        final value = int.parse(cleaned, radix: 16);
        final color = Color(0xFF000000 | value);
        final hsv = HSVColor.fromColor(color);
        setState(() {
          _hue = hsv.hue;
          _saturation = hsv.saturation;
          _value = hsv.value;
        });
      } catch (_) {}
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: AppTheme.current.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: 360),
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text("PICK COLOUR", style: TextStyle(color: AppTheme.current.accent, fontSize: 12, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
              SizedBox(height: 16),

              // Color preview
              Container(
                width: double.infinity,
                height: 60,
                decoration: BoxDecoration(
                  color: _currentColor,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.3)),
                ),
              ),
              SizedBox(height: 16),

              // Hue
              Row(
                children: [
                  Text("H", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12, fontWeight: FontWeight.bold)),
                  SizedBox(width: 8),
                  Expanded(
                    child: SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        activeTrackColor: _currentColor,
                        inactiveTrackColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                        thumbColor: _currentColor,
                        overlayColor: _currentColor.withValues(alpha: 0.2),
                      ),
                      child: Slider(
                        value: _hue,
                        min: 0,
                        max: 360,
                        onChanged: (v) => setState(() => _hue = v),
                      ),
                    ),
                  ),
                ],
              ),

              // Saturation
              Row(
                children: [
                  Text("S", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12, fontWeight: FontWeight.bold)),
                  SizedBox(width: 8),
                  Expanded(
                    child: SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        activeTrackColor: _currentColor,
                        inactiveTrackColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                        thumbColor: _currentColor,
                        overlayColor: _currentColor.withValues(alpha: 0.2),
                      ),
                      child: Slider(
                        value: _saturation,
                        min: 0,
                        max: 1,
                        onChanged: (v) => setState(() => _saturation = v),
                      ),
                    ),
                  ),
                ],
              ),

              // Value (Brightness)
              Row(
                children: [
                  Text("V", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 12, fontWeight: FontWeight.bold)),
                  SizedBox(width: 8),
                  Expanded(
                    child: SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        activeTrackColor: _currentColor,
                        inactiveTrackColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                        thumbColor: _currentColor,
                        overlayColor: _currentColor.withValues(alpha: 0.2),
                      ),
                      child: Slider(
                        value: _value,
                        min: 0,
                        max: 1,
                        onChanged: (v) => setState(() => _value = v),
                      ),
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),

              // Hex input
              TextField(
                controller: _hexController,
                style: TextStyle(color: AppTheme.current.text, fontSize: 13, fontFamily: 'monospace'),
                textAlign: TextAlign.center,
                inputFormatters: [FilteringTextInputFormatter.allow(RegExp(r'[0-9A-Fa-f#]')), LengthLimitingTextInputFormatter(7)],
                decoration: InputDecoration(
                  hintText: "#FF0000",
                  hintStyle: TextStyle(color: AppTheme.current.mutedText.withValues(alpha: 0.5)),
                  filled: true,
                  fillColor: AppTheme.current.text.withValues(alpha: 0.05),
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(8), borderSide: BorderSide.none),
                  contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                ),
                onChanged: _updateFromHex,
              ),
              SizedBox(height: 16),

              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: () => Navigator.pop(context),
                    child: Text("CANCEL", style: TextStyle(color: AppTheme.current.mutedText)),
                  ),
                  SizedBox(width: 8),
                  ElevatedButton(
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.current.accent,
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                    ),
                    onPressed: () => widget.onColorSelected(_currentColor),
                    child: Text("SELECT", style: TextStyle(color: Colors.black, fontWeight: FontWeight.bold)),
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
