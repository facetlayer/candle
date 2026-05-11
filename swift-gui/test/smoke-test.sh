#!/bin/bash
# Smoke test for the swift-gui macOS app.
#
# Builds the .app bundle, launches it, verifies the process starts, captures
# a screenshot, and tears it down. Requires the candle GUI API to be running
# (see ../README.md). Override the API URL with CANDLE_API_URL.
#
# NOTE: idb (the iOS Debug Bridge) does not work on macOS apps. For real UI
# automation we use osascript + screencapture, or you can write an XCUITest
# target in Xcode against the .app bundle.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Candle.app"
EXEC_NAME="SwiftGui"
SHOT_DIR="${SHOT_DIR:-$ROOT/test/screenshots}"
API_URL="${CANDLE_API_URL:-http://localhost:4022/api}"

mkdir -p "$SHOT_DIR"

echo "==> Building app"
"$ROOT/scripts/build-app.sh" release >/dev/null

echo "==> Verifying API is reachable at $API_URL"
if ! curl -sf "$API_URL/services" >/dev/null; then
    echo "ERROR: Candle GUI API not responding at $API_URL/services" >&2
    echo "Start it with: cd $(dirname "$ROOT")/gui && candle start api" >&2
    exit 1
fi

echo "==> Killing any existing instances"
pkill -f "$EXEC_NAME" 2>/dev/null || true
sleep 1

echo "==> Launching app"
CANDLE_API_URL="$API_URL" open "$APP"
sleep 3

if ! pgrep -f "$EXEC_NAME" >/dev/null; then
    echo "FAIL: App did not start" >&2
    exit 1
fi
echo "    PID: $(pgrep -f "$EXEC_NAME")"

echo "==> Positioning window"
osascript >/dev/null 2>&1 <<'EOF' || true
tell application "System Events"
    tell process "SwiftGui"
        set frontmost to true
        delay 0.5
        try
            set position of window 1 to {100, 100}
            set size of window 1 to {1100, 700}
        end try
    end tell
end tell
EOF
sleep 1

echo "==> Capturing screenshot"
SHOT="$SHOT_DIR/launch.png"
screencapture -x -R "100,100,1100,700" "$SHOT"
if [ ! -s "$SHOT" ]; then
    echo "FAIL: screenshot not created" >&2
    pkill -f "$EXEC_NAME" || true
    exit 1
fi
echo "    Screenshot: $SHOT ($(stat -f%z "$SHOT") bytes)"

echo "==> Tearing down"
pkill -f "$EXEC_NAME" 2>/dev/null || true

echo
echo "PASS: app built, launched, fetched services from API, and rendered."
