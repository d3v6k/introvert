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
    _applyColorsFromImage(original);
  }

  void _applyColorsFromImage(img.Image image) {
    final colors = _extractThemeColors(image);
    if (colors.isNotEmpty) {
      setState(() {
        _bg = colors['bg']!;
        _surface = colors['surface']!;
        _text = colors['text']!;
        _mutedText = colors['mutedText']!;
        _accent = colors['accent']!;
        _isDark = _luminance([_bg.red, _bg.green, _bg.blue]) < 128;
      });
    }
  }

  Map<String, Color> _extractThemeColors(img.Image image) {
    // 1. Sample pixels (every 5th for performance)
    final pixels = <List<int>>[];
    for (int y = 0; y < image.height; y += 5) {
      for (int x = 0; x < image.width; x += 5) {
        final p = image.getPixel(x, y);
        pixels.add([p.r.toInt(), p.g.toInt(), p.b.toInt()]);
      }
    }
    if (pixels.isEmpty) return {};

    // 2. Cluster by quantizing to 16-level RGB buckets
    final Map<String, _ColorCluster> clusters = {};
    for (final p in pixels) {
      final key = '${p[0] >> 4}_${p[1] >> 4}_${p[2] >> 4}';
      clusters.putIfAbsent(key, () => _ColorCluster()).add(p);
    }

    // 3. Sort by frequency (most dominant first)
    final sorted = clusters.values.toList()
      ..sort((a, b) => b.count.compareTo(a.count));

    // 4. Dominant color
    final dominant = sorted.first.center;
    final dHsl = _rgbToHsl(dominant[0], dominant[1], dominant[2]);

    // 5. Most vibrant cluster (skip very dark/light, min 2% frequency)
    List<int> vibrant = dominant;
    double maxSat = dHsl[1];
    for (final c in sorted.take(20)) {
      final hsl = _rgbToHsl(c.center[0], c.center[1], c.center[2]);
      if (hsl[2] < 0.15 || hsl[2] > 0.85) continue;
      if (hsl[1] > maxSat && c.count > pixels.length * 0.02) {
        maxSat = hsl[1];
        vibrant = c.center;
      }
    }
    final vHsl = _rgbToHsl(vibrant[0], vibrant[1], vibrant[2]);

    // 6. ACCENT: Analogous shift (+30 degrees), boosted saturation
    final accent = _hslToRgb(
      (vHsl[0] + 30) % 360,
      (vHsl[1] * 1.2).clamp(0.0, 1.0),
      vHsl[2].clamp(0.45, 0.75),
    );

    // 7. BG: Dominant hue, very dark, desaturated
    final bgL = 0.06 + (dHsl[2] * 0.08);
    final bg = _hslToRgb(dHsl[0], (dHsl[1] * 0.6).clamp(0.0, 1.0), bgL);

    // 8. SURFACE: Same hue family, slightly lighter than bg
    final surface = _hslToRgb(
      dHsl[0],
      (dHsl[1] * 0.4).clamp(0.0, 1.0),
      (bgL + 0.08).clamp(0.0, 0.25),
    );

    // 9. TEXT: Near-white with subtle dominant hue tint
    final text = _hslToRgb(dHsl[0], 0.08, 0.95);

    // 10. MUTED TEXT: Mid luminance, very low saturation
    final muted = _hslToRgb(dHsl[0], 0.05, 0.55);

    return {
      'bg': Color.fromARGB(255, bg[0], bg[1], bg[2]),
      'surface': Color.fromARGB(255, surface[0], surface[1], surface[2]),
      'text': Color.fromARGB(255, text[0], text[1], text[2]),
      'mutedText': Color.fromARGB(255, muted[0], muted[1], muted[2]),
      'accent': Color.fromARGB(255, accent[0], accent[1], accent[2]),
    };
  }

  List<double> _rgbToHsl(int r, int g, int b) {
    final rf = r / 255.0, gf = g / 255.0, bf = b / 255.0;
    final max = [rf, gf, bf].reduce((a, c) => a > c ? a : c);
    final min = [rf, gf, bf].reduce((a, c) => a < c ? a : c);
    final d = max - min;
    double h = 0, s = 0;
    final l = (max + min) / 2;
    if (d != 0) {
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
      if (max == rf) h = ((gf - bf) / d + (gf < bf ? 6 : 0)) * 60;
      else if (max == gf) h = ((bf - rf) / d + 2) * 60;
      else h = ((rf - gf) / d + 4) * 60;
    }
    return [h, s, l];
  }

  List<int> _hslToRgb(double h, double s, double l) {
    if (s == 0) { final v = (l * 255).round().clamp(0, 255); return [v, v, v]; }
    final q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    final p = 2 * l - q;
    final hue = h / 360;
    double t(int n) {
      var k = (hue + n / 3) % 1.0;
      if (k < 0) k += 1.0;
      if (k < 1 / 6) return p + (q - p) * 6 * k;
      if (k < 1 / 2) return q;
      if (k < 2 / 3) return p + (q - p) * (2 / 3 - k) * 6;
      return p;
    }
    return [(t(0) * 255).round().clamp(0, 255), (t(1) * 255).round().clamp(0, 255), (t(2) * 255).round().clamp(0, 255)];
  }

  double _luminance(List<int> c) => 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];

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
              Row(
                children: [
                  Text("COLOURS", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 11, fontWeight: FontWeight.bold, letterSpacing: 1)),
                  if (_wallpaperPath != null && File(_wallpaperPath!).existsSync()) ...[
                    SizedBox(width: 8),
                    GestureDetector(
                      onTap: () async {
                        final bytes = await File(_wallpaperPath!).readAsBytes();
                        final image = img.decodeImage(bytes);
                        if (image != null) _applyColorsFromImage(image);
                      },
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(Icons.auto_awesome, color: AppTheme.current.accent, size: 14),
                          SizedBox(width: 3),
                          Text('Auto-Generate', style: TextStyle(color: AppTheme.current.accent, fontSize: 10, fontWeight: FontWeight.w600)),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
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

class _ColorCluster {
  int count = 0;
  int _r = 0, _g = 0, _b = 0;
  void add(List<int> c) { _r += c[0]; _g += c[1]; _b += c[2]; count++; }
  List<int> get center => [_r ~/ count, _g ~/ count, _b ~/ count];
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
