#!/bin/zsh
# usage: drive.sh scenario doc [limit-seconds] [ENV=VAL ...]
# Runs one benchmark scenario in the bundled release app via `open`, so it is a
# normal foreground app, and prints what the driver reported.
R=${0:A:h:h}; A=$R/target/FumiGPUI.app; s=$1; d=$2; lim=${3:-120}; shift 3 2>/dev/null
f=$R/results/$s-$d.log; rm -f $f $f.err
envs=(); for kv in "$@"; do envs+=(--env $kv); done
timeout $lim open -n -W --env FUMI_DRIVE=$s $envs --stdout $f --stderr $f.err $A --args $R/fixtures/$d.md; rc=$?
pkill -f MacOS/FumiGPUI
echo "=== $s $d rc=$rc"; grep -E '^(OPEN|RESULT|CHECK|MEMORY|KEY|FAIL|JUMP)' $f | cut -c1-400; tail -2 $f.err | cut -c1-200
