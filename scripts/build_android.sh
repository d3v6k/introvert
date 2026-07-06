#!/bin/bash

# --- Introvert Android Cross-Compilation Pipeline ---
# This script builds optimized .so libraries for arm64-v8a and x86_64 architectures.

set -e

# Configuration - Auto-detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}"
    HOST_TAG="darwin-x86_64"
else
    ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
    HOST_TAG="linux-x86_64"
fi

NDK_PATH="$ANDROID_SDK_ROOT/ndk/28.2.13676358"
export ANDROID_HOME="$ANDROID_SDK_ROOT"
export ANDROID_NDK_HOME="$NDK_PATH"
export MAKE="/usr/bin/make"
export CMAKE_TOOLCHAIN_FILE="$NDK_PATH/build/cmake/android.toolchain.cmake"
export PATH="$NDK_PATH/toolchains/llvm/prebuilt/$HOST_TAG/bin:$PATH"
export PATH="$(pwd)/scripts/bin:$PATH"

NDK_VERSION="24"
TARGET_ARM="aarch64-linux-android"
TARGET_X64="x86_64-linux-android"
JNI_DIR="android/app/src/main/jniLibs"
LIB_NAME="libintrovert.so"

# Terminal Formatting
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}🚀 Initializing Introvert Android Build Pipeline...${NC}"

# 1. Validation Phase
echo -e "${CYAN}🔍 Validating environment and tooling...${NC}"

if ! command -v cargo-ndk &> /dev/null; then
    echo -e "${RED}❌ cargo-ndk not found. Installing...${NC}"
    cargo install cargo-ndk
fi

echo -e "${GREEN}✅ cargo-ndk is present.${NC}"

echo -e "${CYAN}📦 Ensuring Rust targets are installed...${NC}"
rustup target add $TARGET_ARM $TARGET_X64
echo -e "${GREEN}✅ Rust targets verified.${NC}"

# 2. Build Phase
build_target() {
    local target=$1
    local arch_dir=$2
    local abi=$3
    
    echo -e "${CYAN}🛠  Building optimized release for ${arch_dir} ($target) [ABI: $abi]...${NC}"
    
    export ANDROID_ABI="$abi"
    cargo ndk --target $target --platform $NDK_VERSION build --release
    
    local src_path="target/$target/release/$LIB_NAME"
    local dest_dir="$JNI_DIR/$arch_dir"
    
    echo -e "${CYAN}📁 Preparing distribution directory: $dest_dir${NC}"
    mkdir -p "$dest_dir"
    
    echo -e "${CYAN}💾 Copying and stripping binary...${NC}"
    # Note: 'strip = true' in Cargo.toml handles most stripping, but we ensure it here.
    cp "$src_path" "$dest_dir/$LIB_NAME"
    
    # Copy libc++_shared.so from NDK (required by Rust's std library)
    local cxx_shared="$NDK_PATH/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/$target/libc++_shared.so"
    if [ -f "$cxx_shared" ]; then
        cp "$cxx_shared" "$dest_dir/"
        echo -e "${GREEN}  ✓ libc++_shared.so bundled${NC}"
    else
        echo -e "${RED}  ⚠ libc++_shared.so not found at: $cxx_shared${NC}"
    fi
    
    # Attempt to use NDK strip if available in PATH, otherwise rely on Cargo's strip
    if command -v llvm-strip &> /dev/null; then
        llvm-strip "$dest_dir/$LIB_NAME"
    fi
    
    echo -e "${GREEN}✅ Successfully built and deployed $arch_dir binary.${NC}"
}

# Execute builds
build_target $TARGET_ARM "arm64-v8a" "arm64-v8a"
build_target $TARGET_X64 "x86_64" "x86_64"

# 3. Finalization
echo -e "${GREEN}✨ Introvert Android Build Complete!${NC}"
echo -e "${CYAN}Artifacts available in:${NC}"
ls -R $JNI_DIR
