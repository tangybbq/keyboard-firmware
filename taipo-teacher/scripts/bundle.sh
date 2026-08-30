#!/bin/bash
# Assemble Taipo Teacher.app from the SwiftPM build.
#
# There is no .xcodeproj on purpose.  A checked-in Xcode project is a large generated file
# that conflicts badly in git, and everything it would give us here -- a bundle, an
# Info.plist, a signature -- is a dozen lines of shell.  Xcode is still perfectly usable for
# editing and debugging: `open Package.swift` opens the package directly.
#
# Usage: scripts/bundle.sh [--release] [--run]
set -euo pipefail

cd "$(dirname "$0")/.."

CONFIG=debug
RUN=no
for arg in "$@"; do
    case "$arg" in
        --release) CONFIG=release ;;
        --run) RUN=yes ;;
        *) echo "unknown argument: $arg" >&2; exit 1 ;;
    esac
done

swift build --configuration "$CONFIG" --product TaipoTeacherApp

BIN=$(swift build --configuration "$CONFIG" --show-bin-path)
APP="$BIN/Taipo Teacher.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN/TaipoTeacherApp" "$APP/Contents/MacOS/Taipo Teacher"

# SwiftPM puts a target's resources in a .bundle beside the binary; the app has to carry
# them, or `Bundle.module` finds nothing at runtime and the chord tables are missing.
for b in "$BIN"/*.bundle; do
    [ -e "$b" ] && cp -R "$b" "$APP/Contents/Resources/"
done

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>Taipo Teacher</string>
    <key>CFBundleDisplayName</key>       <string>Taipo Teacher</string>
    <key>CFBundleIdentifier</key>        <string>org.davidb.taipo-teacher</string>
    <key>CFBundleExecutable</key>        <string>Taipo Teacher</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1</string>
    <key>CFBundleVersion</key>           <string>1</string>
    <key>LSMinimumSystemVersion</key>    <string>14.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <!-- Menu bar only: the collector runs in the background, and the practice window is
         opened from the menu when it is wanted. -->
    <key>LSUIElement</key>               <true/>
</dict>
</plist>
PLIST

# Ad-hoc signing is enough: the app is unsandboxed and local, so claiming the vendor USB
# interface needs no entitlement.  The spike established that as an unsigned binary.
codesign --force --sign - "$APP" 2>/dev/null || true

echo "Built: $APP"
[ "$RUN" = yes ] && open "$APP"
exit 0
