def fix_hash(main_path):
    with open(main_path, "r") as f:
        content = f.read()

    # The incorrect hash from thought block vs the actual sha256 hash of "introvert_rbn"
    content = content.replace("c08e5e8e81561a067087093226a27e7d95393282245b73678ad9ab9bfd397e5a", "ac26da29d37bfa455a2697dc7d4179addeb1a2cc4fa1e113275948df823ace25")
    
    with open(main_path, "w") as f:
        f.write(content)
    print(f"Fixed hash in {main_path}")

fix_hash("/Users/dev/Development/introvert/src/main.rs")
fix_hash("/Users/dev/Development/introvert/for_linux/src/main.rs")
