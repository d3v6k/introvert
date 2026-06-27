import os
import shutil
import sys

def copy_recursive(src, dst, ignore_patterns=None):
    if not os.path.exists(src):
        print(f"Warning: source path does not exist: {src}")
        return
        
    if os.path.isdir(src):
        # Ignore target/, build/, .dart_tool/, etc.
        ignored = []
        if ignore_patterns:
            for item in os.listdir(src):
                if any(pat in item for pat in ignore_patterns):
                    ignored.append(item)
                    
        os.makedirs(dst, exist_ok=True)
        for item in os.listdir(src):
            if ignore_patterns and any(pat in item for pat in ignore_patterns):
                continue
            copy_recursive(os.path.join(src, item), os.path.join(dst, item), ignore_patterns)
    else:
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
        print(f"Copied: {src} -> {dst}")

def main():
    src_root = "/Users/dev/Development/introvert"
    dst_root = "/Volumes/512-SSD-External/introvert back up/stable_v40"
    
    print(f"Starting Stable Backup to {dst_root}...")
    
    # 1. Create target directory
    os.makedirs(dst_root, exist_ok=True)
    
    # Ignore build/temp targets to keep the archive clean
    global_ignores = ["target", "build", ".dart_tool", ".gradle", ".idea", ".git", "Pods", "ephemeral", "ios/Flutter/Flutter.podspec"]
    
    # 2. Copy Entire directories recursively
    dirs_to_copy = ["lib", "src", "android", "for_linux", "docs", "tests"]
    for d in dirs_to_copy:
        copy_recursive(os.path.join(src_root, d), os.path.join(dst_root, d), global_ignores)
        
    # 3. Copy root configuration/build files
    files_to_copy = [
        "pubspec.yaml",
        "pubspec.lock",
        "analysis_options.yaml",
        "Cargo.toml",
        "Cargo.lock",
        "Makefile",
        "README.md",
        "GEMINI.md",
        "INTROVERT_MASTER_PLAN.md",
        "deploy_local_rbn.sh",
        "deploy_rbn.sh",
        "introvertd.service"
    ]
    for f in files_to_copy:
        copy_recursive(os.path.join(src_root, f), os.path.join(dst_root, f))
        
    # 4. Copy stable files in root (*.stable)
    for f in os.listdir(src_root):
        if f.endswith(".stable"):
            copy_recursive(os.path.join(src_root, f), os.path.join(dst_root, f))
            
    print("Stable Backup Completed Successfully!")

if __name__ == "__main__":
    main()
