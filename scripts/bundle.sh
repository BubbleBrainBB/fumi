#!/bin/zsh
# Wraps the release binary in a minimal .app: macOS treats a bare binary as a
# background tool in places that matter here (activation, screenshots, input).
R=${0:A:h:h}; A=$R/target/FumiGPUI.app
mkdir -p $A/Contents/MacOS
cp $R/target/release/fumi-gpui $A/Contents/MacOS/FumiGPUI
cat > $A/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>FumiGPUI</string>
<key>CFBundleIdentifier</key><string>com.fumi.gpui-prototype</string>
<key>CFBundleExecutable</key><string>FumiGPUI</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPhotoLibraryAddUsageDescription</key><string>Fumi adds the Xiaohongshu cards you export to Photos, so they reach your phone.</string>
</dict></plist>
P
codesign --force -s - $A >/dev/null 2>&1
echo $A
