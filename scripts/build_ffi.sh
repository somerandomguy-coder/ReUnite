#!/usr/bin/env bash
# Build the Rust mesh core and install it where the Flutter app will look for it.
#
#   ./scripts/build_ffi.sh macos      laptop app (also used by `flutter run -d macos`)
#   ./scripts/build_ffi.sh android    phone app  (installs .so into jniLibs)
#   ./scripts/build_ffi.sh ios        iPhone app (static library, see docs/MOBILE.md)
#
# Everything above the radio - routing, crypto, the node actor - is the same code the
# `meshnet` CLI runs. This script only produces the platform binaries for it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MOBILE="$ROOT/mobile"
TARGET="${1:-macos}"

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

case "$TARGET" in
  macos)
    say "building meshffi for macOS"
    cargo build --release -p meshffi --manifest-path "$ROOT/Cargo.toml"
    DEST="$HOME/.reunite/lib"
    mkdir -p "$DEST"
    cp "$ROOT/target/release/libmeshffi.dylib" "$DEST/"
    say "installed $DEST/libmeshffi.dylib"
    ;;

  android)
    say "building meshffi for Android (arm64, armv7, x86_64)"
    command -v cargo-ndk >/dev/null 2>&1 || {
      echo "cargo-ndk missing. Install it with: cargo install cargo-ndk" >&2
      exit 1
    }
    : "${ANDROID_NDK_HOME:=$(ls -d "$HOME"/Library/Android/sdk/ndk/* 2>/dev/null | sort -V | tail -1)}"
    [ -d "${ANDROID_NDK_HOME:-}" ] || {
      echo "No Android NDK found. Install one in Android Studio, or set ANDROID_NDK_HOME." >&2
      exit 1
    }
    export ANDROID_NDK_HOME
    say "using NDK $ANDROID_NDK_HOME"
    DEST="$MOBILE/android/app/src/main/jniLibs"
    mkdir -p "$DEST"
    ( cd "$ROOT" && cargo ndk \
        -t arm64-v8a -t armeabi-v7a -t x86_64 \
        -o "$DEST" \
        build --release -p meshffi )
    say "installed .so files under $DEST"
    find "$DEST" -name 'libmeshffi.so' -exec ls -lh {} \;
    ;;

  ios)
    say "building meshffi for iOS (device + simulator)"
    cargo build --release -p meshffi --target aarch64-apple-ios --manifest-path "$ROOT/Cargo.toml"
    cargo build --release -p meshffi --target aarch64-apple-ios-sim --manifest-path "$ROOT/Cargo.toml"
    DEST="$MOBILE/ios/Frameworks"
    mkdir -p "$DEST"
    cp "$ROOT/target/aarch64-apple-ios/release/libmeshffi.a" "$DEST/libmeshffi-device.a"
    cp "$ROOT/target/aarch64-apple-ios-sim/release/libmeshffi.a" "$DEST/libmeshffi-sim.a"
    say "static libraries in $DEST"
    cat <<'NOTE'

iOS needs two manual Xcode steps, which cannot be scripted safely:

  1. Open mobile/ios/Runner.xcworkspace, select the Runner target ->
     Build Phases -> Link Binary With Libraries -> add the .a for your destination.
  2. Build Settings -> "Dead Code Stripping" = No, and add
     -all_load to Other Linker Flags, so the C entry points survive the linker.

See docs/MOBILE.md for why, and for the multicast entitlement iOS requires.
NOTE
    ;;

  *)
    echo "usage: $0 [macos|android|ios]" >&2
    exit 1
    ;;
esac

say "done"
