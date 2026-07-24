# Introvert Native Build System

# iPhone 13 Pro device ID — update this if the device changes
IOS_DEVICE ?= 00008110-0009451C1AC2801E

.PHONY: help mac macos-dmg android ios ios-device ios-install clean all bk

help:
	@echo "Introvert Build System"
	@echo "Usage:"
	@echo "  make mac              - Build native library for macOS"
	@echo "  make macos-dmg        - Build macOS app and create DMG installer"
	@echo "  make android          - Build native libraries for Android (arm64 and x64)"
	@echo "  make ios              - Build native static libraries for iOS device + simulator"
	@echo "  make ios-device       - Build Flutter iOS release app"
	@echo "  make ios-install      - Build and install on iPhone 13 Pro (IOS_DEVICE=$(IOS_DEVICE))"
	@echo "  make all              - Build for all platforms (mac + android + ios + ios-device)"
	@echo "  make bk               - Comprehensive backup to external SSD"
	@echo "  make clean            - Remove build artifacts"

mac:
	@echo "🍏 Building macOS Native Core..."
	@cargo build --release > build_mac.log 2>&1 || (cat build_mac.log && exit 1)
	@cp target/release/libintrovert.dylib .
	@mkdir -p macos/Flutter/ephemeral
	@cp target/release/libintrovert.dylib macos/Flutter/ephemeral/libintrovert.dylib
	@echo "✅ macOS build complete. Artifact moved to project root and macos/Flutter/ephemeral/"

macos-dmg:
	@echo "💿 Building macOS DMG..."
	@chmod +x scripts/build_macos_dmg.sh
	@./scripts/build_macos_dmg.sh

android:
	@echo "🤖 Building Android Native Core..."
	@chmod +x scripts/build_android.sh
	@./scripts/build_android.sh > build_android.log 2>&1 || (cat build_android.log && exit 1)
	@echo "✅ Android build complete. Artifacts moved to android/app/src/main/jniLibs/"

ios:
	@echo "🍎 Building iOS Native Core..."
	@IPHONEOS_DEPLOYMENT_TARGET=13.0 cargo build --release --target aarch64-apple-ios > build_ios_device.log 2>&1 || (cat build_ios_device.log && exit 1)
	@IPHONEOS_DEPLOYMENT_TARGET=13.0 cargo build --release --target aarch64-apple-ios-sim > build_ios_sim.log 2>&1 || (cat build_ios_sim.log && exit 1)
	@mkdir -p ios/libs
	@cp target/aarch64-apple-ios/release/libintrovert.a ios/libs/libintrovert_device.a
	@cp target/aarch64-apple-ios-sim/release/libintrovert.a ios/libs/libintrovert_simulator.a
	@echo "✅ iOS build complete. Libraries available in ios/libs/"

ios-device: ios
	@echo "📱 Building Flutter iOS release app..."
	@flutter build ios --release 2>&1 | tail -3
	@echo "✅ Flutter iOS release app built."

ios-install: ios-device
	@echo "📲 Installing on iPhone 13 Pro ($(IOS_DEVICE))..."
	@xcrun devicectl device install app --device $(IOS_DEVICE) build/ios/iphoneos/Runner.app 2>&1
	@echo "✅ Installed on device."

all: mac android ios ios-device

clean:
	cargo clean

bk:
	@echo "💾 Running comprehensive backup..."
	@chmod +x scripts/backup.sh
	@./scripts/backup.sh
