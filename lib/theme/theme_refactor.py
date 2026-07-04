import os
import re
import glob

def refactor_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    original_content = content

    # Add import if not present
    if "import '../../theme/app_theme.dart';" not in content and "import '../theme/app_theme.dart';" not in content and filepath != "lib/theme/app_theme.dart":
        # Calculate relative path
        depth = len(filepath.split('/')) - 2
        rel_path = '../' * depth + 'theme/app_theme.dart'
        if rel_path.startswith('./'): rel_path = rel_path[2:]
        if rel_path == "": rel_path = "theme/app_theme.dart"
        
        # Insert import after the last import
        imports = re.findall(r"^import\s+.*;$", content, flags=re.MULTILINE)
        if imports:
            last_import = imports[-1]
            content = content.replace(last_import, f"{last_import}\nimport '{rel_path}';")
        else:
            content = f"import '{rel_path}';\n" + content

    # Define color replacements
    replacements = [
        (r'const\s+Color\(0xFF0A0E17\)', 'AppTheme.current.bg'),
        (r'Color\(0xFF0A0E17\)', 'AppTheme.current.bg'),
        (r'const\s+Color\(0xFF0F1219\)', 'AppTheme.current.bg'),
        (r'Color\(0xFF0F1219\)', 'AppTheme.current.bg'),
        (r'const\s+Color\(0xFF0A0E14\)', 'AppTheme.current.bg'),
        (r'Color\(0xFF0A0E14\)', 'AppTheme.current.bg'),
        
        (r'const\s+Color\(0xFF1A1F2B\)', 'AppTheme.current.surface'),
        (r'Color\(0xFF1A1F2B\)', 'AppTheme.current.surface'),
        (r'const\s+Color\(0xFF121620\)', 'AppTheme.current.surface'),
        (r'Color\(0xFF121620\)', 'AppTheme.current.surface'),
        (r'const\s+Color\(0xFF0F131D\)', 'AppTheme.current.surface'),
        (r'Color\(0xFF0F131D\)', 'AppTheme.current.surface'),
        
        (r'Colors\.cyanAccent', 'AppTheme.current.accent'),
        (r'const\s+Color\(0xFF00FFFF\)', 'AppTheme.current.accent'),
        (r'Color\(0xFF00FFFF\)', 'AppTheme.current.accent'),
        (r'const\s+Color\(0xFF1AFFFF\)', 'AppTheme.current.accent'),
        (r'Color\(0xFF1AFFFF\)', 'AppTheme.current.accent'),

        (r'Colors\.white70', 'AppTheme.current.text.withOpacity(0.7)'),
        (r'Colors\.white54', 'AppTheme.current.mutedText'),
        (r'Colors\.white38', 'AppTheme.current.mutedText.withOpacity(0.7)'),
        (r'Colors\.white24', 'AppTheme.current.mutedText.withOpacity(0.5)'),
        (r'Colors\.white12', 'AppTheme.current.mutedText.withOpacity(0.2)'),
        (r'Colors\.white10', 'AppTheme.current.mutedText.withOpacity(0.1)'),
        (r'Colors\.white\.withOpacity', 'AppTheme.current.text.withOpacity'),
        (r'Colors\.white', 'AppTheme.current.text'),
    ]

    for pattern, repl in replacements:
        content = re.sub(pattern, repl, content)

    # Strip 'const' from widgets and decorations where themes might be used
    widgets_to_unconst = [
        'TextStyle', 'BoxDecoration', 'BorderSide', 'Icon', 'Text',
        'Center', 'Padding', 'SizedBox', 'CircularProgressIndicator',
        'Divider', 'CircleAvatar', 'Row', 'Column', 'Align',
        'Container', 'Scaffold', 'AppBar', 'ListTile', 'EdgeInsets',
        'Border', 'ThemeData', 'NavigationBarThemeData', 'TextSpan', 'RichText',
        'InputDecoration', 'UnderlineInputBorder', 'OutlineInputBorder',
        'SnackBar', 'AlertDialog'
    ]
    for w in widgets_to_unconst:
        content = re.sub(r'const\s+(' + w + r'\(|' + w + r'\.)', r'\1', content)
        content = re.sub(r'const\s+(<[^>]+>\s*\[)', r'\1', content) # const <Widget>[ -> <Widget>[
        content = re.sub(r'const\s+\[', r'[', content) # const [ -> [

    if content != original_content:
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"Updated {filepath}")

# Find all dart files
files = glob.glob('lib/**/*.dart', recursive=True)
for f in files:
    if not f.startswith('lib/theme/') and not f.endswith('theme_mockup_grid.dart'):
        refactor_file(f)

