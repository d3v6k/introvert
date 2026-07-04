#!/bin/bash
# CMake wrapper to force Android ABI and API level during cross-compilation

# Find the real cmake
REAL_CMAKE="/usr/bin/cmake"

ARGS=()
for arg in "$@"; do
    ARGS+=("$arg")
done

# If building for Android, inject the missing mandatory flags
if [[ "$*" == *"DCMAKE_SYSTEM_NAME=Android"* ]] || [[ "$*" == *"DCMAKE_TOOLCHAIN_FILE="* ]]; then
    # Use environment variables set in build_android.sh
    if [ -n "$ANDROID_ABI" ]; then
        ARGS+=("-DANDROID_ABI=$ANDROID_ABI")
    fi
    if [ -n "$NDK_VERSION" ]; then
        ARGS+=("-DANDROID_PLATFORM=android-$NDK_VERSION")
    fi
    # Force Unix Makefiles if not set
    if [[ "$*" != *"G"* ]]; then
         ARGS+=("-G" "Unix Makefiles")
    fi
fi

exec "$REAL_CMAKE" "${ARGS[@]}"
