#!/bin/sh
# Builds the iOS app for the booted simulator, installs it and launches it with any launch arguments, such as
# `-pair <ticket>` to pair with a computer (from `recall host pair`) or `-screen review` (see `Scripted`).
#   apps/ios/scripts/simulate.sh [launch arguments…]    e.g. -pair rchub1… -screen settings
set -eu

ios="$(cd "$(dirname "$0")/.." && pwd)"
bundle=co.anomaly.recall
build="$ios/build/xcode-sim"

cd "$ios"
xcodegen generate --quiet
xcodebuild -project Recall.xcodeproj -scheme Recall -configuration Debug \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath "$build" CODE_SIGNING_ALLOWED=NO \
  -quiet build
xcrun simctl install booted "$build/Build/Products/Debug-iphonesimulator/Recall.app"
xcrun simctl launch --terminate-running-process booted "$bundle" "$@"
