#!/bin/bash
# End-to-end test driving the swift-gui UI via the debug HTTP server.
#
# Pattern copied from ~/tools/facetlayer-desktop/docs/introspection.md.
# Builds, launches with the debug server enabled, then walks through:
#   1. Verify initial state (no selection)
#   2. Select a service
#   3. Verify selection + logs loaded
#   4. Toggle auto-scroll
#   5. Capture screenshot via /screen
#   6. Deselect
#   7. Tear down

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Candle.app"
EXEC_NAME="SwiftGui"
SHOT_DIR="${SHOT_DIR:-$ROOT/test/screenshots}"
API_URL="${CANDLE_API_URL:-http://localhost:4022/api}"
DEBUG_PORT="${CANDLE_DEBUG_PORT:-4044}"
CLI="$ROOT/bin/debug-api.ts"

mkdir -p "$SHOT_DIR"

fail() { echo "FAIL: $*" >&2; pkill -f "$EXEC_NAME" 2>/dev/null || true; exit 1; }

echo "==> Build"
"$ROOT/scripts/build-app.sh" release >/dev/null

echo "==> Verify Candle GUI API is up at $API_URL"
curl -sf "$API_URL/services" >/dev/null || fail "Candle GUI API not reachable at $API_URL"

echo "==> Launch app with debug server"
pkill -f "$EXEC_NAME" 2>/dev/null || true
sleep 1
CANDLE_DEBUG_SERVER=1 CANDLE_DEBUG_PORT="$DEBUG_PORT" CANDLE_API_URL="$API_URL" open "$APP"

echo "==> Wait for debug server"
"$CLI" wait 15 >/dev/null

echo "==> Initial state"
INITIAL=$("$CLI" state)
SVC_COUNT=$(echo "$INITIAL" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["services"]))')
SELECTED=$(echo "$INITIAL" | python3 -c 'import sys,json; print(json.load(sys.stdin)["selected"])')
echo "    services: $SVC_COUNT"
echo "    selected: $SELECTED"
[ "$SVC_COUNT" -gt 0 ] || fail "no services from API"
[ "$SELECTED" = "None" ] || fail "expected no initial selection, got $SELECTED"

echo "==> Pick first running service for selection"
FIRST=$(echo "$INITIAL" | python3 -c '
import sys, json
s = json.load(sys.stdin)
for svc in s["services"]:
    if svc["isRunning"]:
        print(svc["serviceName"], svc["projectDir"])
        break
')
read -r PICK_NAME PICK_DIR <<< "$FIRST"
echo "    pick: $PICK_NAME ($PICK_DIR)"

echo "==> Select"
"$CLI" select "$PICK_NAME" "$PICK_DIR" >/dev/null
SELECT_STATE=$("$CLI" state)
NEW_SEL=$(echo "$SELECT_STATE" | python3 -c "import sys,json; s=json.load(sys.stdin)['selected']; print(s['serviceName'] if s else '')")
[ "$NEW_SEL" = "$PICK_NAME" ] || fail "selection didn't stick (got '$NEW_SEL')"
echo "    selected: $NEW_SEL"

echo "==> Refresh logs"
"$CLI" refresh-logs

echo "==> Toggle auto-scroll off"
"$CLI" auto-scroll off >/dev/null
AS=$("$CLI" state | python3 -c 'import sys,json; print(json.load(sys.stdin)["autoScroll"])')
[ "$AS" = "False" ] || fail "auto-scroll didn't toggle (got $AS)"
echo "    autoScroll: $AS"

echo "==> Capture screenshot via /screen"
SHOT="$SHOT_DIR/debug-server.png"
"$CLI" screen "$SHOT" >/dev/null
[ -s "$SHOT" ] || fail "screen capture empty"
SIZE=$(stat -f%z "$SHOT")
echo "    $SHOT ($SIZE bytes)"

echo "==> Toggle auto-scroll back on"
"$CLI" auto-scroll on >/dev/null

echo "==> Deselect"
"$CLI" select --clear >/dev/null
CLEARED=$("$CLI" state | python3 -c 'import sys,json; print(json.load(sys.stdin)["selected"])')
[ "$CLEARED" = "None" ] || fail "deselect failed (got $CLEARED)"

echo "==> Tear down"
pkill -f "$EXEC_NAME" 2>/dev/null || true

echo
echo "PASS: built, launched, drove the UI via debug server, and captured pixels."
