import 'dart:io';
import 'package:image/image.dart' as img;

void main() {
  final countries = {
    'france': 'France',
    'brazil': 'Brazil',
    'USA': 'USA',
    'mexico': 'Mexico',
    'argentina': 'Argentina',
    'england': 'England',
    'italy': 'Italy',
    'colombia': 'Colombia',
    'senegal': 'Senegal',
    'uruguay': 'Uruguay',
    'croatia': 'Croatia',
    'belgium': 'Belgium',
    'netherlands': 'Netherlands',
    'morocco': 'Morocco',
    'canada': 'Canada',
    'germany': 'Germany',
    'spain': 'Spain',
    'portugal': 'Portugal',
  };

  final srcDir = '/Users/dev/Documents/introvert logo/theme images';
  final dstDir = '/Users/dev/Development/introvert/assets/images/fifa';
  final results = <String, Map<String, dynamic>>{};

  for (final entry in countries.entries) {
    final file = File('$srcDir/${entry.key}.png');
    if (!file.existsSync()) {
      print('SKIP: ${entry.key}.png not found');
      continue;
    }

    final bytes = file.readAsBytesSync();
    final original = img.decodeImage(bytes);
    if (original == null) {
      print('ERROR: Could not decode ${entry.key}.png');
      continue;
    }

    // Resize to 720px width, encode as JPEG quality 80
    final resized = img.copyResize(original, width: 720, interpolation: img.Interpolation.linear);
    final jpgBytes = img.encodeJpg(resized, quality: 80);
    File('$dstDir/${entry.key}.jpg').writeAsBytesSync(jpgBytes);
    final sizeKB = (jpgBytes.length / 1024).round();

    // Extract colors using same algorithm as custom_theme_creator
    final colors = _extractThemeColors(original);
    results[entry.key] = {'name': entry.value, 'colors': colors, 'sizeKB': sizeKB};
    print('✅ ${entry.value}: ${resized.width}x${resized.height}, ${sizeKB}KB');
    final bg = colors['bg'] ?? 0;
    final surf = colors['surface'] ?? 0;
    final acc = colors['accent'] ?? 0;
    print('   bg=0x${bg.toRadixString(16).padLeft(8, '0')} '
        'surface=0x${surf.toRadixString(16).padLeft(8, '0')} '
        'accent=0x${acc.toRadixString(16).padLeft(8, '0')}');
  }

  // Output ThemeConfig code
  print('\n// === PASTE INTO app_theme.dart themes list ===\n');
  print('    // FIFA World Cup 2026');
  for (final entry in results.entries) {
    final c = entry.value['colors'] as Map<String, int>;
    final name = entry.value['name'];
    final slug = entry.key;
    final bg = (c['bg'] ?? 0).toRadixString(16).padLeft(8, '0');
    final surf = (c['surface'] ?? 0).toRadixString(16).padLeft(8, '0');
    final txt = (c['text'] ?? 0).toRadixString(16).padLeft(8, '0');
    final muted = (c['mutedText'] ?? 0).toRadixString(16).padLeft(8, '0');
    final acc = (c['accent'] ?? 0).toRadixString(16).padLeft(8, '0');
    print('''
    ThemeConfig(
      name: "FIFA $name",
      bg: Color(0x$bg),
      surface: Color(0x$surf),
      text: Color(0x$txt),
      mutedText: Color(0x$muted),
      accent: Color(0x$acc),
      isDark: true,
      wallpaperPath: "assets/images/fifa/$slug.jpg",
      wallpaperOpacity: 1.0,
    ),''');
  }
}

Map<String, int> _extractThemeColors(img.Image image) {
  final pixels = <List<int>>[];
  for (int y = 0; y < image.height; y += 5) {
    for (int x = 0; x < image.width; x += 5) {
      final p = image.getPixel(x, y);
      pixels.add([p.r.toInt(), p.g.toInt(), p.b.toInt()]);
    }
  }
  if (pixels.isEmpty) return {};

  final Map<String, _Cluster> clusters = {};
  for (final p in pixels) {
    final key = '${p[0] >> 4}_${p[1] >> 4}_${p[2] >> 4}';
    clusters.putIfAbsent(key, () => _Cluster()).add(p);
  }
  final sorted = clusters.values.toList()
    ..sort((a, b) => b.count.compareTo(a.count));

  final dominant = sorted.first.center;
  final dHsl = _rgbToHsl(dominant[0], dominant[1], dominant[2]);

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

  final accent = _hslToRgb((vHsl[0] + 30) % 360, (vHsl[1] * 1.2).clamp(0.0, 1.0), vHsl[2].clamp(0.45, 0.75));
  final bgL = 0.06 + (dHsl[2] * 0.08);
  final bg = _hslToRgb(dHsl[0], (dHsl[1] * 0.6).clamp(0.0, 1.0), bgL);
  final surface = _hslToRgb(dHsl[0], (dHsl[1] * 0.4).clamp(0.0, 1.0), (bgL + 0.08).clamp(0.0, 0.25));
  final text = _hslToRgb(dHsl[0], 0.08, 0.95);
  final muted = _hslToRgb(dHsl[0], 0.05, 0.55);

  return {
    'bg': _argb(255, bg[0], bg[1], bg[2]),
    'surface': _argb(255, surface[0], surface[1], surface[2]),
    'text': _argb(255, text[0], text[1], text[2]),
    'mutedText': _argb(255, muted[0], muted[1], muted[2]),
    'accent': _argb(255, accent[0], accent[1], accent[2]),
  };
}

int _argb(int a, int r, int g, int b) => (a << 24) | (r << 16) | (g << 8) | b;

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

class _Cluster {
  int count = 0;
  int _r = 0, _g = 0, _b = 0;
  void add(List<int> c) { _r += c[0]; _g += c[1]; _b += c[2]; count++; }
  List<int> get center => [_r ~/ count, _g ~/ count, _b ~/ count];
}
