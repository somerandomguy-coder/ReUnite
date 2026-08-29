#!/usr/bin/env bash
#
# Every check the project has, in one command. Non-zero exit if any of them fails.
#
#   ./scripts/check.sh          run everything
#   ./scripts/check.sh rust     only the Rust workspace
#   ./scripts/check.sh dart     only the Flutter app
#
# The Dart widget tests drive a real Rust node through the FFI bridge, so they need
# the core built for this machine. This script builds it if it is missing rather than
# failing with a dlopen error that reads like a bug in the app.

set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
WHICH="${1:-all}"
FAILED=()

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
run()  { if "${@:2}"; then :; else FAILED+=("$1"); fi; }

if [[ "$WHICH" == "all" || "$WHICH" == "rust" ]]; then
  step "cargo test --workspace"
  run "cargo test" cargo test --workspace --no-fail-fast

  step "cargo build --release"
  run "cargo build" cargo build --release
fi

if [[ "$WHICH" == "all" || "$WHICH" == "dart" ]]; then
  if ! command -v flutter >/dev/null 2>&1; then
    echo "flutter not on PATH - skipping the Dart checks"
  else
    LIB="$ROOT/target/release/libmeshffi.dylib"
    [[ "$(uname)" == "Linux" ]] && LIB="$ROOT/target/release/libmeshffi.so"
    if [[ ! -f "$LIB" ]]; then
      step "building the mesh core for the widget tests"
      cargo build --release -p meshffi
    fi

    cd "$ROOT/mobile"

    step "flutter analyze"
    # --fatal-warnings so a warning cannot accumulate quietly into the next phase.
    run "flutter analyze" flutter analyze --fatal-warnings

    step "flutter test"
    run "flutter test" env MESHFFI_LIB="$LIB" flutter test

    cd "$ROOT"
  fi
fi

printf '\n'
if (( ${#FAILED[@]} )); then
  printf '\033[31mFAILED:\033[0m %s\n' "${FAILED[*]}"
  exit 1
fi
printf '\033[32mall checks passed\033[0m\n'
