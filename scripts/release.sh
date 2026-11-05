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

shasum -a 256 $DMG
