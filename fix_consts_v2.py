import re
import glob

def remove_const_on_lines_with_apptheme(filepath):
    with open(filepath, 'r') as f:
        lines = f.readlines()
    
    new_lines = []
    for line in lines:
        if 'AppTheme.current' in line:
            # Simple dumb const removal for the whole line
            line = line.replace('const ', '')
            
            # Additional targeted unconst for multi-line issues reported
            if 'AlwaysStoppedAnimation' in line:
                line = line.replace('AlwaysStoppedAnimation', 'AlwaysStoppedAnimation')
        new_lines.append(line)

    with open(filepath, 'w') as f:
        f.writelines(new_lines)

# Fix network optimization button specifically
def fix_network_button():
    path = 'lib/src/ui/widgets/network_optimization_button.dart'
    with open(path, 'r') as f:
        content = f.read()
    content = content.replace('this.color,', 'this.color,')
    content = re.sub(r'final\s+Color\s+color\s*;\s*', r'final Color? color;\n', content)
    # in build method, fallback to AppTheme.current.accent
    content = content.replace('color: color,', 'color: color ?? AppTheme.current.accent,')
    content = content.replace('color: color.withOpacity', 'color: (color ?? AppTheme.current.accent).withOpacity')
    
    # We must remove any remaining 'const' in front of NetworkOptimizationButton calls
    with open(path, 'w') as f:
        f.write(content)

# Fix chat_features.dart TabBar const issues
def fix_chat_features():
    path = 'lib/views/chat_features.dart'
    with open(path, 'r') as f:
        content = f.read()
    # remove const from TabBar
    content = content.replace('const TabBar(', 'TabBar(')
    with open(path, 'w') as f:
        f.write(content)

# Execute
files = glob.glob('lib/**/*.dart', recursive=True)
for f in files:
    remove_const_on_lines_with_apptheme(f)

fix_network_button()
fix_chat_features()
