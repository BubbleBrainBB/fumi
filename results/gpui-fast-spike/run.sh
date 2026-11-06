#!/bin/zsh
# Full-app BENCH_SCENARIO runs, alternating builds, each on a fresh library copy.
S=${0:A:h:h}; B=$S/bench; O=$B/out; mkdir -p $O
FAST=/Users/chengshengdeng/bubble/notes-app/.claude/worktrees/recursing-hermann-5df520/target/FumiGPUI.app
BASE=$S/base/target/FumiGPUI.app
fresh(){ rm -rf $B/lib; cp -R $B/lib.orig $B/lib; }
one(){ # name app scenario doc run [env]
  fresh; f=$O/$1-$3-$4-$5.log; x=(); [[ -n $6 ]] && x=(--env $6)
  timeout 150 open -n -W $x --env FUMI_WORKSPACE=$B/lib --env BENCH_LIB=$B/lib --env BENCH_SCENARIO=$3 --env BENCH_DOC=$4 --stdout $f --stderr $f.err $2
  pkill -f MacOS/FumiGPUI; sleep 1
}
for r in ${=RUNS:-1 2 3}; do
  for sd in ${=CASES:-launch:none open:none type:bench200k type:bench1m scroll:bench1m idle:bench200k}; do
    s=${sd%%:*}; d=${sd##*:}
    echo "--- r$r $s $d load $(sysctl -n vm.loadavg)"
    one base $BASE $s $d $r; one fast $FAST $s $d $r
    [[ -n $NORET ]] && one fastnr $FAST $s $d $r GPUI_VIEW_RETENTION=0
    [[ -n $NOLAYER ]] && one fastnl $FAST $s $d $r GPUI_SCROLL_LAYERS=0
  done
done
rm -rf $B/lib; echo ALLDONE
