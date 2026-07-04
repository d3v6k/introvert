# Release Process

## Overview

This document outlines the process for creating and publishing Introvert releases.

## Release Types

### Major Release (x.0.0)
- Breaking changes
- New architecture
- Database migrations required
- Full regression testing

### Minor Release (0.x.0)
- New features
- Bug fixes
- Performance improvements
- Backward compatible

### Patch Release (0.0.x)
- Critical bug fixes
- Security patches
- Documentation updates
- No new features

## Pre-Release Checklist

### Code Quality
- [ ] All tests passing
- [ ] No compiler warnings
- [ ] Code review completed
- [ ] Security audit (for major releases)

### Documentation
- [ ] CHANGELOG.md updated
- [ ] README.md updated (if needed)
- [ ] API documentation updated
- [ ] Release notes drafted

### Testing
- [ ] Unit tests passing
- [ ] Integration tests passing
- [ ] Manual testing completed
- [ ] Performance benchmarks acceptable

### Build
- [ ] Android APK built and tested
- [ ] iOS IPA built and tested
- [ ] macOS app built and tested
- [ ] Linux RBN binary built and tested

## Release Steps

### 1. Update Version Numbers
```bash
# pubspec.yaml
version: 1.0.0+1

# Cargo.toml
version = "1.0.0"

# android/app/build.gradle.kts
versionCode = 1
versionName = "1.0.0"
```

### 2. Update CHANGELOG.md
```markdown
## [1.0.0] - 2026-06-14

### Added
- Feature A
- Feature B

### Changed
- Improvement A

### Fixed
- Bug fix A

### Security
- Security patch A
```

### 3. Create Git Tag
```bash
git add .
git commit -m "chore: release v1.0.0"
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin main --tags
```

### 4. Build Release Binaries
```bash
# Android (split per ABI)
flutter build apk --release --target-platform android-arm64 --split-per-abi

# iOS
flutter build ios --release

# macOS
flutter build macos --release

# Linux RBN
cargo build --release --bin introvertd
```

### 5. Test Release Builds
```bash
# Android
adb install build/app/outputs/flutter-apk/app-arm64-v8a-release.apk

# iOS (via Xcode)
# macOS
open build/macos/Build/Products/Release/introvert.app

# Linux
./target/release/introvertd --help
```

### 6. Create GitHub Release
1. Go to GitHub Releases
2. Click "Draft a new release"
3. Select tag `v1.0.0`
4. Title: `v1.0.0`
5. Description: Copy from CHANGELOG.md
6. Attach binaries:
   - `app-arm64-v8a-release.apk`
   - `app-x86_64-release.apk`
   - `introvert.dmg` (macOS)
   - `introvertd-linux-x86_64` (Linux RBN)
7. Publish release

### 7. Deploy RBN Update
```bash
# Cross-compile for Linux
cargo zigbuild --target x86_64-unknown-linux-gnu --release --bin introvertd

# Deploy to RBN
./deploy_local_rbn.sh
```

### 8. Post-Release
- [ ] Verify RBN is running new version
- [ ] Monitor for issues
- [ ] Update documentation site
- [ ] Announce release (Twitter, Discord, etc.)

## Version Numbering

Follow Semantic Versioning:
- **MAJOR:** Incompatible API changes
- **MINOR:** Backward-compatible new functionality
- **PATCH:** Backward-compatible bug fixes

## Database Migrations

If schema changes are required:
1. Add migration in `src/storage.rs`
2. Test migration on existing data
3. Document migration in CHANGELOG.md
4. Include rollback procedure

## Rollback Procedure

If critical issues are found:
1. Revert to previous version tag
2. Rebuild binaries
3. Deploy previous version to RBN
4. Notify users to downgrade
5. Fix issue and release patch

## Release Schedule

- **Major releases:** As needed (quarterly target)
- **Minor releases:** Monthly
- **Patch releases:** As needed (critical fixes)

## Communication

### Before Release
- Announce on Discord
- Tweet preview
- Update documentation

### After Release
- Publish release notes
- Update social media
- Monitor issue tracker
- Respond to user feedback
