# Introvert Native Build System
# Supports macOS (local), Android (cross-compile), and iOS (cross-compile)

.PHONY: help mac android ios clean all

help:
	@echo "Introvert Build System"
	@echo "Usage:"
	@echo "  make mac      - Build native library for macOS"
	@echo "  make android  - Build native libraries for Android (arm64 and x64)"
	@echo "  make ios      - Build native static libraries for iOS (device and simulator)"
	@echo "  make all      - Build for all platforms"
	@echo "  make clean    - Remove build artifacts"

mac:
	@echo "🍏 Building macOS Native Core..."
	@cargo build --release > build_mac.log 2>&1 || (cat build_mac.log && exit 1)
	@cp target/release/libintrovert.dylib .
	@mkdir -p macos/Flutter/ephemeral
	@cp target/release/libintrovert.dylib macos/Flutter/ephemeral/libintrovert.dylib
	@echo "✅ macOS build complete. Artifact moved to project root and macos/Flutter/ephemeral/"

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

all: mac android ios

clean:
	cargo clean
	rm -rf android/app/src/main/jniLibs/*
	rm -rf ios/libs/*
	rm -f libintrovert.dylib
	rm -f build_mac.log build_android.log build_ios_device.log build_ios_sim.log
	@echo "🧹 Workspace cleaned."
