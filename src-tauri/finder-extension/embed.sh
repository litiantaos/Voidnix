#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
EXT_BUNDLE_ID="com.litiantao.voidnix.FinderExt"

# ---------- Detect signing identity ---------------------------------------
SIGN_IDENTITY=""
DEV_CERT=$(security find-identity -v -p codesigning 2>/dev/null \
    | grep "Apple Development" | head -1 | sed 's/.*"\(.*\)".*/\1/')
if [ -n "$DEV_CERT" ]; then
    SIGN_IDENTITY="$DEV_CERT"
    echo "==> Using signing identity: $SIGN_IDENTITY"
else
    SIGN_IDENTITY="-"
    echo "==> No Apple Development certificate found, using ad-hoc signing."
fi

# ---------- Locate app bundle ---------------------------------------------
# The first argument may be:
#   - "debug"   → $PROJECT_DIR/target/debug/bundle/macos/Voidnix.app
#   - "release" → $PROJECT_DIR/target/release/bundle/macos/Voidnix.app  (default)
#   - an absolute path to a .app bundle
TARGET_ARG="${1:-release}"
if [[ "$TARGET_ARG" == /* ]]; then
    APP_PATH="$TARGET_ARG"
elif [ "$TARGET_ARG" = "debug" ]; then
    APP_PATH="$PROJECT_DIR/target/debug/bundle/macos/Voidnix.app"
else
    APP_PATH="$PROJECT_DIR/target/release/bundle/macos/Voidnix.app"
fi

if [ ! -d "$APP_PATH" ]; then
    echo "==> Voidnix.app not found at $APP_PATH"
    echo "    Run 'bun run tauri build' first."
    exit 1
fi

BINARY="$SCRIPT_DIR/FinderExt"
APPEX_DIR="$SCRIPT_DIR/FinderExt.appex"

trap 'rm -f "$BINARY"; rm -rf "$APPEX_DIR"' EXIT

# ---------- Compile extension ---------------------------------------------
echo "==> Compiling Finder Sync extension..."
clang -fobjc-arc \
    -framework Foundation \
    -framework AppKit \
    -framework FinderSync \
    -mmacosx-version-min=11.0 \
    -o "$BINARY" \
    "$SCRIPT_DIR/FinderSync.m"

# ---------- Stage the .appex bundle ---------------------------------------
echo "==> Creating .appex bundle..."
rm -rf "$APPEX_DIR"
mkdir -p "$APPEX_DIR/Contents/MacOS"
cp "$BINARY" "$APPEX_DIR/Contents/MacOS/FinderExt"
cp "$SCRIPT_DIR/Info.plist" "$APPEX_DIR/Contents/Info.plist"

# ---------- Sign the staged .appex ----------------------------------------
echo "==> Signing extension (Hardened Runtime + entitlements)..."
codesign --force --sign "$SIGN_IDENTITY" \
    --options runtime \
    --entitlements "$SCRIPT_DIR/FinderExt.entitlements" \
    "$APPEX_DIR"

# ---------- Kill any running extension instances before replacing ----------
# pluginkit/Finder keeps old copies of the extension alive. Replacing the
# binary on disk without killing them leaves zombies and breaks the next
# launch. Use || true because the process may not be running.
echo "==> Terminating any running Finder extension instances..."
killall -9 FinderExt 2>/dev/null || true

# ---------- Embed into app bundle -----------------------------------------
echo "==> Embedding extension into $APP_PATH..."
PLUGINS_DIR="$APP_PATH/Contents/PlugIns"
mkdir -p "$PLUGINS_DIR"
rm -rf "$PLUGINS_DIR/FinderExt.appex"
cp -r "$APPEX_DIR" "$PLUGINS_DIR/"

echo "==> Re-signing extension in place..."
codesign --force --sign "$SIGN_IDENTITY" \
    --options runtime \
    --entitlements "$SCRIPT_DIR/FinderExt.entitlements" \
    "$PLUGINS_DIR/FinderExt.appex"

# ---------- Re-sign the main app ------------------------------------------
# The main app is non-sandboxed. We only need Hardened Runtime so the
# sandboxed extension (which requires runtime) can be hosted underneath it.
# No App Group, no extra TCC entitlements — IPC is a plain directory under
# ~/Library/Application Support/Voidnix/commands/.
APP_ENTITLEMENTS="$SCRIPT_DIR/App.entitlements"
cat > "$APP_ENTITLEMENTS" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.app-sandbox</key>
	<false/>
</dict>
</plist>
EOF

echo "==> Re-signing application bundle (Hardened Runtime)..."
codesign --force --sign "$SIGN_IDENTITY" \
    --options runtime \
    --entitlements "$APP_ENTITLEMENTS" \
    "$APP_PATH"

rm -f "$APP_ENTITLEMENTS"

# ---------- Refresh pluginkit registration --------------------------------
echo "==> Refreshing pluginkit registration..."

# Remove by path (pluginkit -r takes a path, not a bundle identifier).
# Old stale path registrations can linger after the app moves.
pluginkit -r "$PLUGINS_DIR/FinderExt.appex" 2>/dev/null || true

# Register the new appex.
if pluginkit -a "$PLUGINS_DIR/FinderExt.appex"; then
    echo "    Extension registered successfully."
else
    echo "==> WARNING: Extension registration failed."
    echo "    Check Console.app for details (filter: 'pluginkit')."
fi

# Kick pluginkit's daemon and Finder so they pick up the new bundle.
# killall Finder is disruptive (closes Finder windows), but it's the only
# reliable way to re-host the extension after signing / entitlement changes.
echo "==> Reloading pkd and Finder to pick up the new extension..."
killall -KILL pkd 2>/dev/null || true
killall Finder 2>/dev/null || true

# ---------- Final Gatekeeper check ----------------------------------------
EXT_BINARY="$PLUGINS_DIR/FinderExt.appex/Contents/MacOS/FinderExt"
if spctl --assess -v "$EXT_BINARY" 2>/dev/null; then
    echo "==> Extension passed Gatekeeper validation."
else
    echo "==> Extension is ad-hoc / dev-signed (expected for local builds)."
fi

# ---------- Status summary ------------------------------------------------
echo ""
echo "==> Done. Final registration state:"
pluginkit -mAvv -p com.apple.FinderSync 2>/dev/null \
    | awk -v id="$EXT_BUNDLE_ID" '
        $0 ~ id { printing = 1 }
        printing { print }
        printing && /^$/ { exit }
    ' || true

cat <<EOT

Next steps:
  1. Launch Voidnix (the main app starts the file watcher).
  2. Open System Settings → General → Login Items & Extensions
     (macOS 26 groups Finder Sync under "File Providers" / "文件提供程序")
     and enable "Voidnix Finder Extension".
  3. Right-click a file/folder inside ~/Desktop, ~/Documents, ~/Downloads,
     ~/Pictures, ~/Movies, or ~/Music to see the menu.
EOT
