#!/bin/zsh
# usage: demo.sh <crate-short-name> [ENV=VAL ...]
# Builds crates/fumi-<name>/examples/demo.rs in release, wraps it in its own .app
# (bundle id com.fumi.demo.<name>), restarts it, and prints the .app path. Each
# crate has its own app, so agents working at once don't kill each other's.
R=${0:A:h:h}; n=$1; shift
cargo build --release --offline -p fumi-$n --example demo 2>&1 | grep -E "^error|^warning: unused|could not compile" -A6 | head -60
A=$R/target/demo-$n.app; E=Demo${(C)n}
mkdir -p $A/Contents/MacOS
pkill -x $E
cp $R/target/release/examples/demo $A/Contents/MacOS/$E
cat > $A/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$E</string>
<key>CFBundleIdentifier</key><string>com.fumi.demo.$n</string>
<key>CFBundleExecutable</key><string>$E</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force -s - $A >/dev/null 2>&1
envs=(); for kv in "$@"; do envs+=(--env $kv); done
open -n $envs $A
echo $A
