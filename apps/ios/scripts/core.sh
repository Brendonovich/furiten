#!/bin/sh
# Builds Recall's core (crates/ffi) as a static library for the iOS app, into build/core/<platform>/librecall_ffi.a.
# Xcode runs it before compiling (project.yml); cargo makes it quick when nothing changed.
#   apps/ios/scripts/core.sh [iphoneos|iphonesimulator]    (default $PLATFORM_NAME, as Xcode sets it, else iphoneos)
set -eu

ios="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$ios/../.." && pwd)"
platform="${1:-${PLATFORM_NAME:-iphoneos}}"
case "$platform" in
  iphoneos) target=aarch64-apple-ios ;;
  iphonesimulator) target=aarch64-apple-ios-sim ;;
  *) echo "core.sh: unknown platform $platform" >&2; exit 1 ;;
esac

# Xcode's build environment (SDKROOT, its compilers and flags) is for the app, and would leak into the C that
# rusqlite and ring compile; build with a clean one. Release: a debug build of Iroh is large and slow on a phone.
env -i HOME="$HOME" PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" \
  IPHONEOS_DEPLOYMENT_TARGET=26.0 \
  cargo build --quiet --manifest-path "$root/Cargo.toml" -p recall-ffi --release --target "$target"
mkdir -p "$ios/build/core/$platform"
cp "$root/target/$target/release/librecall_ffi.a" "$ios/build/core/$platform/librecall_ffi.a"
