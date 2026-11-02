#!/bin/bash
# Builds a Release of Fumi, signs it for distribution outside the App Store
# (Developer ID, hardened runtime) and packs it into a signed DMG in build/release.
#
#   Tools/release.sh              build, sign and pack
#   Tools/release.sh --notarize   also send the DMG to Apple and staple the ticket
#
# --notarize needs a notarytool profile in the keychain, stored once with:
#   xcrun notarytool store-credentials fumi --apple-id <id> --team-id 2Y5QK5UNBM
#
# SIGN_IDENTITY and NOTARY_PROFILE override the defaults below.
set -euo pipefail

cd "$(dirname "$0")/.."

IDENTITY="${SIGN_IDENTITY:-Developer ID Application: Chengsheng Deng (2Y5QK5UNBM)}"
PROFILE="${NOTARY_PROFILE:-fumi}"
ENTITLEMENTS="Tools/Notely.entitlements"
DERIVED="build/rel"
OUT="build/release"
APP="$DERIVED/Build/Products/Release/Fumi.app"

NOTARIZE=0
case "${1:-}" in
  "") ;;
  --notarize) NOTARIZE=1 ;;
  *) echo "usage: Tools/release.sh [--notarize]" >&2; exit 2 ;;
esac

# What goes to Apple should match a commit.
if [ "$NOTARIZE" = 1 ] && [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "Commit your changes before notarizing." >&2
  exit 1
fi

echo "==> Building"
xcodebuild -project Notely.xcodeproj -scheme Notely -configuration Release \
  -derivedDataPath "$DERIVED" clean build -quiet

VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist")
ARCH=$(lipo -archs "$APP/Contents/MacOS/Fumi" | tr ' ' '-')
DMG="$OUT/Fumi-$VERSION-$ARCH.dmg"

echo "==> Signing Fumi $VERSION ($ARCH)"
codesign --force --options runtime --timestamp \
  --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP"
codesign --verify --deep --strict "$APP"

echo "==> Packing $DMG"
mkdir -p "$OUT"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/Fumi.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -ov -volname "Fumi" -fs HFS+ -format UDZO -srcfolder "$STAGE" "$DMG"
codesign --force --timestamp --sign "$IDENTITY" "$DMG"

if [ "$NOTARIZE" = 1 ]; then
  echo "==> Notarizing"
  xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
  xcrun stapler staple "$DMG"
  spctl --assess --type open --context context:primary-signature --verbose "$DMG"
fi

shasum -a 256 "$DMG"
