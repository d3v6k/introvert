import os
import re

root_dir = "/Users/dev/Development/introvert"

replacements = [
    # 1. RBN Requirements (50,000 -> 2,000,000)
    (r"50,000\s*\$INTR", "2,000,000 $INTR"),
    (r"50,000\s*INTR", "2,000,000 INTR"),
    (r"50,000\s*\$intr", "2,000,000 $INTR"),
    (r"50\s*K\s*INTR", "2M INTR"),
    (r"50k\s*INTR", "2M INTR"),
    (r"50K\s*INTR", "2M INTR"),
    (r"50k\s*\$INTR", "2M $INTR"),
    
    # 2. Edge Node Requirements (500 -> 100,000)
    (r"500\s*\$INTR", "100,000 $INTR"),
    (r"500\s*INTR", "100,000 INTR"),
    (r"500INTR", "100kINTR"),
    (r"wallet_balance\s*<\s*500", "wallet_balance < 100000"),
    (r"balance\s*<\s*500", "balance < 100000"),
    
    # 3. Multiplier (38 -> 3)
    (r"38×", "3×"),
    (r"38x", "3x"),
    (r"38\.0", "3.0"),
    (r"38-fold", "3-fold"),
]

modified_files = []

# Exclude these directories
exclude_dirs = {".git", ".cargo", "target", "build", ".mimocode", "ios", "android", "macos", "linux", "web", "windows", "stable_v12", "stable_v35"}

for root, dirs, files in os.walk(root_dir):
    # Prune excluded directories
    dirs[:] = [d for d in dirs if d not in exclude_dirs]
    
    for file in files:
        if file.endswith((".md", ".txt")):
            filepath = os.path.join(root, file)
            with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
            
            new_content = content
            made_change = False
            
            for pattern, replacement in replacements:
                compiled = re.compile(pattern, re.IGNORECASE)
                if compiled.search(new_content):
                    new_content = compiled.sub(replacement, new_content)
                    made_change = True
            
            if made_change:
                with open(filepath, "w", encoding="utf-8") as f:
                    f.write(new_content)
                modified_files.append(filepath)
                print(f"Updated: {filepath}")

print(f"\nSuccessfully updated {len(modified_files)} files in the project.")
