import re
import glob

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Fix typo getters
    content = content.replace('AppTheme.current.text30', 'AppTheme.current.text.withOpacity(0.3)')
    content = content.replace('AppTheme.current.text60', 'AppTheme.current.text.withOpacity(0.6)')

    # Strip const from more widgets and constructors
    words = ['NetworkOptimizationButton', 'PopupMenuItem', 'Expanded', 'AlwaysStoppedAnimation', 'VerticalDivider']
    for w in words:
        content = re.sub(r'const\s+(' + w + r'\(|' + w + r'<)', r'\1', content)

    # Special case in network_optimization_button.dart
    if 'network_optimization_button.dart' in filepath:
        content = re.sub(r'this\.color\s*=\s*AppTheme\.current\.accent,', r'this.color,', content)

    with open(filepath, 'w') as f:
        f.write(content)

files = glob.glob('lib/**/*.dart', recursive=True)
for f in files:
    fix_file(f)
