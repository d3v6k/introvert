import re
import glob

def remove_const_on_lines_with_apptheme(filepath):
    with open(filepath, 'r') as f:
        lines = f.readlines()
    
    new_lines = []
    for line in lines:
        if 'AppTheme.current' in line:
            line = line.replace('const ', '')
        new_lines.append(line)

    with open(filepath, 'w') as f:
        f.writelines(new_lines)

# Execute
files = glob.glob('lib/**/*.dart', recursive=True)
for f in files:
    remove_const_on_lines_with_apptheme(f)
