# Testing Guide

## Overview

Introvert uses a multi-layered testing approach:
1. **Unit Tests** — Individual function testing
2. **Integration Tests** — Component interaction testing
3. **Manual Tests** — User experience validation
4. **Stress Tests** — Performance and scalability

## Running Tests

### Rust Unit Tests
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

### Flutter Unit Tests
```bash
# Run all tests
flutter test

# Run specific test
flutter test test/widget_test.dart

# Run with coverage
flutter test --coverage
```

### Integration Tests
```bash
# Android
flutter test integration_test/

# iOS
flutter test integration_test/ --device-id <device_id>
```

## Test Structure

### Rust Tests (`tests/`)
```
tests/
├── integration_test.rs    # Full engine tests
├── storage_test.rs        # Database operations
├── network_test.rs        # Network layer tests
├── crypto_test.rs         # Encryption tests
└── embedding_test.rs      # Intro-Claw semantic intent engine (16/16 passing)
```

### Flutter Tests (`test/`)
```
test/
├── unit/
│   ├── introvert_client_test.dart
│   └── identity_manager_test.dart
├── widget/
│   ├── main_shell_test.dart
│   └── drive_tab_test.dart
└── integration/
    └── messaging_test.dart
```

## Writing Tests

### Rust Unit Test
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_derivation() {
        let seed = [0u8; 32];
        let identity = NodeIdentity::from_seed(seed).unwrap();
        assert!(!identity.peer_id.to_string().is_empty());
    }

    #[test]
    fn test_storage_operations() {
        let storage = StorageService::new_ephemeral().unwrap();
        storage.store_message("peer1", "msg1", "Hello", true).unwrap();
        let messages = storage.get_messages("peer1").unwrap();
        assert_eq!(messages.len(), 1);
    }
}
```

### Flutter Unit Test
```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:introvert_tests/src/native/introvert_client.dart';

void main() {
  group('IntrovertClient', () {
    test('generates valid mnemonic', () {
      final client = IntrovertClient();
      final mnemonic = client.generateMnemonic();
      expect(mnemonic.split(' ').length, 24);
    });

    test('derives valid seed from mnemonic', () {
      final client = IntrovertClient();
      final mnemonic = client.generateMnemonic();
      final seed = client.mnemonicToSeed(mnemonic);
      expect(seed.length, 32);
    });
  });
}
```

### Widget Test
```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:introvert_tests/blueprint_ui.dart';

void main() {
  testWidgets('SovereignAvatar renders correctly', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignAvatar(
            initials: 'AB',
            radius: 20,
          ),
        ),
      ),
    );

    expect(find.text('AB'), findsOneWidget);
  });
}
```

## Test Coverage

### Checking Coverage
```bash
# Rust
cargo tarpaulin --out Html

# Flutter
flutter test --coverage
genhtml coverage/lcov.info -o coverage/html
```

### Coverage Goals
- **Unit Tests:** >80% code coverage
- **Integration Tests:** All critical paths
- **Widget Tests:** All UI components

## Manual Testing Checklist

### First Launch
- [ ] App launches without crash
- [ ] Onboarding flow completes
- [ ] Seed generation works
- [ ] Identity derivation succeeds

### Messaging
- [ ] Send message to contact
- [ ] Receive message from contact
- [ ] Read receipts update
- [ ] Reactions work
- [ ] Reply threading works

### File Transfer
- [ ] Send file to contact
- [ ] Receive file from contact
- [ ] Progress tracking works
- [ ] Hash verification passes
- [ ] File opens correctly

### Groups
- [ ] Create new group
- [ ] Add members
- [ ] Send group message
- [ ] Receive group message
- [ ] Remove member works

### Calls
- [ ] Initiate voice call
- [ ] Initiate video call
- [ ] Accept incoming call
- [ ] Reject incoming call
- [ ] Call quality acceptable

### Sovereign Drive
- [ ] Upload file to drive
- [ ] Download file from drive
- [ ] Delete file from drive
- [ ] Search files works

## Intro-Claw Test Coverage

### Embedding Engine Tests
```bash
# Run Intro-Claw embedding tests
cargo test test_embedding

# Run Intro-Claw automation tick tests
cargo test test_intro_claw_tick
```

- **16/16 tests passing** for the semantic intent engine
- Covers: model loading, intent classification, cosine similarity, keyword fallback, query parsing

## Stress Testing

### Network Stress
```bash
# Run mesh stress test
cargo test --release --test stress_test -- --test-threads=1
```

### File Transfer Stress
```bash
# Test large file transfers
for i in {1..10}; do
  dd if=/dev/urandom of=test_$i.bin bs=1M count=100
done
```

## Continuous Integration

### GitHub Actions
```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test
      - uses: subosito/flutter-action@v2
      - run: flutter test
```

### Pre-commit Hooks
```bash
# Install pre-commit
pip install pre-commit

# Add hooks
pre-commit install
```

## Bug Reports

### Required Information
1. **Platform:** Android/iOS/macOS/Linux
2. **OS Version:** e.g., Android 14, iOS 17
3. **Steps to Reproduce:** Clear, numbered steps
4. **Expected Behavior:** What should happen
5. **Actual Behavior:** What actually happens
6. **Logs:** Relevant error messages
7. **Screenshots:** If applicable

### Template
```markdown
## Bug Report

**Platform:** 
**OS Version:** 
**App Version:** 

### Steps to Reproduce
1. 
2. 
3. 

### Expected Behavior


### Actual Behavior


### Logs
```

### Screenshots
```
```
```
