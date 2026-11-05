#!/bin/zsh
# Runs each scenario on the Swift app (FumiBench.app, Swift 0.3.2 sources plus
# Autopilot.swift) and on the Rust app (FumiGPUI.app plus src/autopilot.rs), one
# after the other, each from a fresh copy of the same library.
B=${0:A:h}; G=$HOME/bubble/fumi-gpui/target/FumiGPUI.app; O=$B/compare; mkdir -p $O
fresh(){ rm -rf $B/libs/lib1000; cp -R $B/libs/lib1000.orig $B/libs/lib1000; }
swift(){ # scenario doc
  fresh; f=$O/swift-$1-$2.log
  defaults delete com.notes.fumi-bench >/dev/null 2>&1; defaults write com.notes.fumi-bench fumi.workspacePath "$B/libs/lib1000"
  timeout 150 open -n -W --env BENCH_LIB=$B/libs/lib1000 --env BENCH_SCENARIO=$1 --env BENCH_DOC=$2 --stdout $f --stderr $f.err $B/FumiBench.app
  for p in $(pgrep -x Fumi); do case "$(ps -o command= -p $p)" in *scratchpad/bench/FumiBench.app*) kill $p;; esac; done
}
rust(){
  fresh; f=$O/rust-$1-$2.log
  timeout 150 open -n -W --env FUMI_WORKSPACE=$B/libs/lib1000 --env BENCH_LIB=$B/libs/lib1000 --env BENCH_SCENARIO=$1 --env BENCH_DOC=$2 --stdout $f --stderr $f.err $G
  pkill -x FumiGPUI
}
both(){ echo "--- $1 $2 load $(sysctl -n vm.loadavg)"; swift $1 $2; rust $1 $2; }
both launch none
both open none
for d in bench200k bench1m; do for s in type slow newline ime caret scroll; do both $s $d; done; done
both idle bench200k
fresh
defaults delete com.notes.fumi-bench >/dev/null 2>&1
echo ALLDONE
