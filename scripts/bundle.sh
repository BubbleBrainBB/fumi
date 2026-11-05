#!/bin/zsh
# Wraps the release binary in a .app: macOS treats a bare binary as a background
# tool in places that matter here (activation, screenshots, input). The bundle says
# what the Swift app's project.yml says. By default it's the test build, with its
# own id (its own settings) and executable; scripts/release.sh sets the shipping
# ones through BUNDLE_ID, BUNDLE_EXEC and BUNDLE_OUT.
set -e
R=${0:A:h:h}
ID=${BUNDLE_ID:-com.fumi.gpui-prototype}
EXEC=${BUNDLE_EXEC:-FumiGPUI}
A=${BUNDLE_OUT:-$R/target/FumiGPUI.app}
ASSETS=$R/resources/AppIcon.appiconset
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' $R/Cargo.toml | head -1)

mkdir -p $A/Contents/MacOS $A/Contents/Resources
rm -rf $A/Contents/MacOS
mkdir -p $A/Contents/MacOS
cp $R/target/release/fumi-gpui $A/Contents/MacOS/$EXEC

# The icon: the asset catalog's sizes, renamed as iconutil expects, or scaled from
# the largest with sips where one is missing.
ICONSET=$(mktemp -d -t fumi-icon).iconset
mkdir -p $ICONSET
trap 'rm -rf $ICONSET ${ICONSET%.iconset}' EXIT
for points in 16 32 128 256 512; do
  for scale in 1 2; do
    pixels=$((points * scale))
    if (( scale == 1 )); then name=icon_${points}x${points}.png; else name=icon_${points}x${points}@2x.png; fi
    if [[ -f $ASSETS/$name ]]; then
      cp $ASSETS/$name $ICONSET/$name
    else
      sips -z $pixels $pixels $ASSETS/icon_512x512@2x.png --out $ICONSET/$name >/dev/null
    fi
  done
done
iconutil -c icns $ICONSET -o $A/Contents/Resources/AppIcon.icns

cat > $A/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fumi</string>
<key>CFBundleDisplayName</key><string>Fumi</string>
<key>CFBundleIdentifier</key><string>$ID</string>
<key>CFBundleExecutable</key><string>$EXEC</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
<key>NSHumanReadableCopyright</key><string>Copyright (c) 2025 Dylan Deng</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticTermination</key><false/>
<key>NSSupportsSuddenTermination</key><false/>
<key>NSPhotoLibraryAddUsageDescription</key><string>Fumi adds the Xiaohongshu cards you export to Photos, so they reach your phone.</string>
</dict></plist>
P
[[ -n $BUNDLE_OUT ]] || codesign --force -s - $A >/dev/null 2>&1
echo $A
