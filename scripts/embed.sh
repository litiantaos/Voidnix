#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Finder extension source files
APPEX_SRC="$PROJECT_DIR/extensions/finder-ext/appex"
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
TARGET_ARG="${1:-release}"
if [[ "$TARGET_ARG" == /* ]]; then
    APP_PATH="$TARGET_ARG"
elif [ "$TARGET_ARG" = "debug" ]; then
    APP_PATH="$PROJECT_DIR/src-tauri/target/debug/bundle/macos/Voidnix.app"
else
    APP_PATH="$PROJECT_DIR/src-tauri/target/release/bundle/macos/Voidnix.app"
fi

if [ ! -d "$APP_PATH" ]; then
    echo "==> Voidnix.app not found at $APP_PATH"
    echo "    Run 'bun run tauri build' first."
    exit 1
fi

BUILD_DIR=$(mktemp -d)
trap 'rm -rf "$BUILD_DIR"' EXIT

BINARY="$BUILD_DIR/FinderExt"
APPEX_DIR="$BUILD_DIR/FinderExt.appex"

# ---------- Compile extension ---------------------------------------------
echo "==> Compiling Finder Sync extension..."
clang++ -fobjc-arc \
    -std=c++17 \
    -framework Foundation \
    -framework AppKit \
    -framework FinderSync \
    -mmacosx-version-min=11.0 \
    -o "$BINARY" \
    "$APPEX_SRC/FinderSync.mm"

# ---------- Stage the .appex bundle ---------------------------------------
echo "==> Creating .appex bundle..."
mkdir -p "$APPEX_DIR/Contents/MacOS"
cp "$BINARY" "$APPEX_DIR/Contents/MacOS/FinderExt"
cp "$APPEX_SRC/Info.plist" "$APPEX_DIR/Contents/Info.plist"

# ---------- Sign the staged .appex ----------------------------------------
echo "==> Signing extension (Hardened Runtime + entitlements)..."
codesign --force --sign "$SIGN_IDENTITY" \
    --options runtime \
    --entitlements "$APPEX_SRC/FinderExt.entitlements" \
    "$APPEX_DIR"

# ---------- Kill any running extension instances before replacing ----------
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
    --entitlements "$APPEX_SRC/FinderExt.entitlements" \
    "$PLUGINS_DIR/FinderExt.appex"

# ---------- Re-sign the main app ------------------------------------------
APP_ENTITLEMENTS="$BUILD_DIR/App.entitlements"
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

# ---------- Refresh pluginkit registration --------------------------------
echo "==> Refreshing pluginkit registration..."
pluginkit -r "$PLUGINS_DIR/FinderExt.appex" 2>/dev/null || true

if pluginkit -a "$PLUGINS_DIR/FinderExt.appex"; then
    echo "    Extension registered successfully."
else
    echo "==> WARNING: Extension registration failed."
    echo "    Check Console.app for details (filter: 'pluginkit')."
fi

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