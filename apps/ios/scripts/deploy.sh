#!/bin/sh
# Builds the iOS app and installs and launches it on an iPhone paired with this Mac (over Wi-Fi works: pair it once
# in Xcode's Devices window with "Connect via network").
#   apps/ios/scripts/deploy.sh [device]    device: name or identifier from `xcrun devicectl list devices`
#                                          (default $RECALL_DEVICE, else the first available iPhone)
set -eu

ios="$(cd "$(dirname "$0")/.." && pwd)"
bundle=co.anomaly.recall
build="$ios/build/xcode"

device="${1:-${RECALL_DEVICE:-}}"
if [ -z "$device" ]; then
  list="$(mktemp)"
  xcrun devicectl list devices --json-output "$list" >/dev/null
  # A paired iPhone on Wi-Fi reads "unavailable" until something opens a tunnel to it, so try connected ones first,
  # then each paired one.
  candidates="$(plutil -extract result.devices json -o - "$list" | /usr/bin/python3 -c '
import json, sys
phones = [d for d in json.load(sys.stdin) if d["hardwareProperties"].get("deviceType") == "iPhone"
          and d["connectionProperties"].get("pairingState") == "paired"]
phones.sort(key=lambda d: d["connectionProperties"].get("tunnelState") != "connected")
for d in phones: print(d["identifier"])
')"
  rm -f "$list"
  for id in $candidates; do
    if xcrun devicectl device info details --device "$id" --timeout 15 2>/dev/null | grep -q "tunnelState: connected"; then
      device="$id"; break
    fi
  done
  [ -n "$device" ] || { echo "deploy: no available iPhone; is it unlocked and on the same network?" >&2; exit 1; }
fi

cd "$ios"
xcodegen generate --quiet
xcodebuild -project Recall.xcodeproj -scheme Recall -configuration Debug \
  -destination 'generic/platform=iOS' -derivedDataPath "$build" -allowProvisioningUpdates \
  -quiet build
app="$build/Build/Products/Debug-iphoneos/Recall.app"
xcrun devicectl device install app --device "$device" "$app"
xcrun devicectl device process launch --device "$device" --terminate-existing "$bundle"
