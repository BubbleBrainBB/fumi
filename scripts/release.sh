#!/bin/zsh
# Builds Fumi for distribution outside the App Store, as the Swift app's
# Tools/release.sh does: the shipping id (com.notes.app, so it reads the settings
# the Swift app left), Developer ID with the hardened runtime, and a signed DMG in
# target/dist.
#
#   scripts/release.sh              build, sign and pack
#   scripts/release.sh --notarize   also send the DMG to Apple and staple the ticket
#
# --notarize uses the notarytool profile the Swift release already stored ("fumi").
# SIGN_IDENTITY and NOTARY_PROFILE override the defaults.
#
# The updater reads a static appcast.json. Release notes come from
# RELEASE_NOTES or RELEASE_NOTES_FILE; the feed and its history are written to
# target/dist/appcast.json. Publish that file to the project's GitHub Pages site
# (the URL fumi-updater reads) alongside uploading the DMG on the release.
set -euo pipefail
R=${0:A:h:h}
cd $R

IDENTITY="${SIGN_IDENTITY:-Developer ID Application: Chengsheng Deng (2Y5QK5UNBM)}"
PROFILE="${NOTARY_PROFILE:-fumi}"
OUT=$R/target/dist
APP=$OUT/stage/Fumi.app

NOTARIZE=0
case "${1:-}" in
  "") ;;
  --notarize) NOTARIZE=1 ;;
  *) echo "usage: scripts/release.sh [--notarize]" >&2; exit 2 ;;
esac

# What goes to Apple should match a commit.
if [[ $NOTARIZE == 1 && -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "Commit your changes before notarizing." >&2
  exit 1
fi

echo "==> Building"
cargo build --release --offline --locked

rm -rf $OUT/stage
mkdir -p $OUT/stage
BUNDLE_ID=com.notes.app BUNDLE_EXEC=Fumi BUNDLE_OUT=$APP scripts/bundle.sh >/dev/null

VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" $APP/Contents/Info.plist)
ARCH=$(lipo -archs $APP/Contents/MacOS/Fumi | tr ' ' '-')
DMG=$OUT/Fumi-$VERSION-$ARCH.dmg

echo "==> Signing Fumi $VERSION ($ARCH)"
codesign --force --options runtime --timestamp \
  --entitlements scripts/Fumi.entitlements --sign "$IDENTITY" $APP
codesign --verify --deep --strict $APP

echo "==> Packing $DMG"
STAGE=$(mktemp -d)
trap 'rm -rf $STAGE' EXIT
ditto $APP $STAGE/Fumi.app
ln -s /Applications $STAGE/Applications
hdiutil create -quiet -ov -volname "Fumi" -fs HFS+ -format UDZO -srcfolder $STAGE $DMG
codesign --force --timestamp --sign "$IDENTITY" $DMG

if [[ $NOTARIZE == 1 ]]; then
  echo "==> Notarizing"
  xcrun notarytool submit $DMG --keychain-profile "$PROFILE" --wait
  xcrun stapler staple $DMG
  spctl --assess --type open --context context:primary-signature --verbose $DMG
fi

SHA256=$(shasum -a 256 $DMG | awk '{print $1}')
echo "$SHA256  $DMG"

# Publish what the updater reads. One entry per version, newest first; an
# existing feed's history is preserved and this version is replaced/inserted.
BUILD_NUMBER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" $APP/Contents/Info.plist)
NOTES="${RELEASE_NOTES:-}"
if [[ -n "${RELEASE_NOTES_FILE:-}" ]]; then NOTES=$(cat "$RELEASE_NOTES_FILE"); fi
DMG_NAME=$(basename "$DMG")
ASSET_URL="https://github.com/BubbleBrainBB/fumi/releases/download/v$VERSION/$DMG_NAME"
APPCAST=$OUT/appcast.json
echo "==> Writing $APPCAST"
python3 - "$APPCAST" "$VERSION" "$BUILD_NUMBER" "$SHA256" "$ASSET_URL" "$NOTES" <<'PY'
import json, os, sys
path, version, build, sha, url, notes = sys.argv[1:7]
try:
    with open(path) as f:
        feed = json.load(f)
except (OSError, ValueError):
    feed = {"versions": []}
versions = [v for v in feed.get("versions", []) if v.get("version") != version]
entry = {
    "version": version,
    "build": int(build) if build.isdigit() else 0,
    "date": __import__("datetime").date.today().isoformat(),
    "notes": notes,
    "asset": {"url": url, "sha256": sha.lower()},
    "min_os": "14.0",
}
versions.insert(0, entry)
with open(path, "w") as f:
    json.dump({"versions": versions}, f, indent=2, ensure_ascii=False)
    f.write("\n")
print("appcast written:", path)
PY
