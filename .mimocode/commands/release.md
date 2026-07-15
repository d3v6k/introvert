---
description: "Build all platforms, collect release artifacts, and optionally upload to GitHub release. Use for preparing a new release or verifying release readiness."
---

# Release Preparation

Build all platforms, collect artifacts into `/tmp/release-artifacts/`, and verify release readiness.

## Arguments

`$ARGUMENTS` — optional: release tag (e.g., `v0.1.0`). Defaults to latest tag.

## Steps

### 1. Full Build Verification

Run the build-verify skill first:
- `cargo check --lib` (Rust client)
- `cargo check` in `for_linux/` (RBN daemon)
- `flutter analyze` (Dart)
- `make all` (native libs: macOS + Android + iOS)
- `flutter build macos --release` + `flutter build apk --release`

### 2. Collect Artifacts

```bash
mkdir -p /tmp/release-artifacts
cd /Users/dev/Development/introvert

# Android APK
cp build/app/outputs/flutter-apk/app-release.apk /tmp/release-artifacts/

# macOS app (zipped)
zip -r /tmp/release-artifacts/Introvert-macOS.zip build/macos/Build/Products/Release/introvert.app

# Linux binary (from thinkpad, if available)
scp dev@thinkpad.local:~/introvert/for_linux/target/release/introvertd /tmp/release-artifacts/ 2>/dev/null || echo "Linux binary not available"
```

### 3. Verify Release Assets

```bash
TAG=${ARGUMENTS:-v0.1.0}
gh release view $TAG --repo d3v6k/introvert --json assets --jq '.assets[] | "\(.name) - \(.size) bytes"' 2>&1
```

### 4. Upload (requires user approval)

**NEVER upload without explicit user approval.** After user confirms:

```bash
TAG=${ARGUMENTS:-v0.1.0}
gh release upload $TAG /tmp/release-artifacts/* --repo d3v6k/introvert --clobber
```

## Rules

- NEVER upload `introvertd` to GitHub releases (server binary).
- NEVER upload without explicit user approval.
- Verify `.gitignore` coverage before any `git push`.
- Only client-side binaries are safe for release: `libintrovert.dylib`, `app-release.apk`, DMG/ZIP, `.a` static libs.
