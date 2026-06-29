# Contributing to Introvert

## Welcome

Thank you for your interest in contributing to Introvert! This document provides guidelines and instructions for contributing.

## Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Prioritize user privacy and security
- Maintain professional communication

## Getting Started

### 1. Fork & Clone
```bash
git clone https://github.com/your-username/introvert.git
cd introvert
```

### 2. Set Up Development Environment
Follow the `REBUILD_GUIDE.md` to install:
- Rust toolchain (1.75+)
- Flutter SDK (3.22+)
- Android NDK (v28.2.13676358)
- Xcode (for iOS/macOS)

### 3. Build & Test
```bash
make mac          # Build native core
flutter pub get   # Install Flutter dependencies
flutter test      # Run Flutter tests
cargo test        # Run Rust tests
```

### 4. Understanding the Architecture
Before contributing, read these key documents:
- `Docs/INTROVERT_MASTER_PLAN.md` — Vision, tech stack, and execution roadmap
- `Docs/ARCHITECTURE_BLUEPRINT.md` — Component layers, escrow pipeline, token sink mechanics
- `Docs/NETWORKING_&_SIGNALING.md` — Dynamic blockchain bootstrapping, Sybil resistance
- `Docs/SECURITY_&_ENCRYPTION.md` — PDA isolation, Squads V4 governance, identity derivation

## Development Workflow

### Branch Naming
- `feature/description` — New features
- `fix/description` — Bug fixes
- `docs/description` — Documentation changes
- `refactor/description` — Code refactoring

### Commit Messages
Use conventional commits:
```
feat: add new group mutation type
fix: resolve FFI memory leak in file transfer
docs: update rebuild guide with iOS instructions
refactor: simplify noise session handling
```

### Pull Request Process

1. **Create a branch** from `main`
2. **Make changes** with clear, focused commits
3. **Test thoroughly** on at least one platform
4. **Update documentation** if adding features
5. **Submit PR** with:
   - Clear title and description
   - Reference to related issues
   - Screenshots/recordings for UI changes
   - Testing instructions

## Code Style

### Rust
- Follow `rustfmt` defaults
- Use `clippy` lints
- Document public functions with `///`
- Handle errors with `anyhow::Result`

### Dart
- Follow `flutter analyze` recommendations
- Use `dart format`
- Document public APIs
- Handle errors gracefully

### General
- No hardcoded secrets or keys
- No `println!` in production code (use `tracing`)
- Minimize `unsafe` blocks
- Use `try/finally` for FFI cleanup

## Testing

### Unit Tests
```bash
# Rust
cargo test

# Flutter
flutter test
```

### Integration Tests
- Test on physical devices when possible
- Verify FFI bridge functionality
- Test network connectivity scenarios

### Manual Testing
- Test on multiple platforms (Android, iOS, macOS)
- Verify file transfer speeds
- Test offline/online transitions
- Verify encryption works correctly

## Security

### Reporting Vulnerabilities
**DO NOT** open public issues for security vulnerabilities.

Instead, email security@introvert.dev with:
- Description of the vulnerability
- Steps to reproduce
- Potential impact assessment
- Suggested fix (if available)

### Security Guidelines
- Never commit secrets, keys, or credentials
- Use environment variables for sensitive config
- Validate all external inputs
- Follow principle of least privilege

## Documentation

### When to Update Docs
- Adding new features
- Changing APIs
- Modifying build process
- Updating configuration options

### Documentation Standards
- Use clear, concise language
- Include code examples
- Test all code snippets
- Keep tables formatted

## Issue Guidelines

### Bug Reports
Include:
- Steps to reproduce
- Expected behavior
- Actual behavior
- Platform/OS version
- Screenshots if applicable

### Feature Requests
Include:
- Problem statement
- Proposed solution
- Alternatives considered
- Use cases

## License

By contributing, you agree that your contributions will be licensed under the project license.

## Questions?

Join our community:
- GitHub Discussions: [link]
- Discord: [link]
- Twitter: [link]
