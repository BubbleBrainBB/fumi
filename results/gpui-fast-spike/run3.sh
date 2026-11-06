#!/bin/zsh
# Pass 3: gpui-pre vs gpui-fast with scroll layers off (set in main), all scenarios.
S=${0:A:h:h}; B=$S/bench; O=$B/out3; mkdir -p $O
FAST=/Users/chengshengdeng/bubble/notes-app/.claude/worktrees/recursing-hermann-5df520/target/FumiGPUI.app
BASE=$S/base/target/FumiGPUI.app
locked(){ ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked | grep -q true; }
one(){ # name app scenario doc run
  rm -rf $B/lib; cp -R $B/lib.orig $B/lib; f=$O/$1-$3-$4-$5.log
  locked && echo "LOCKED-BEFORE" > $f.lock
  timeout 150 open -n -W --env FUMI_WORKSPACE=$B/lib --env BENCH_LIB=$B/lib --env BENCH_SCENARIO=$3 --env BENCH_DOC=$4 --stdout $f --stderr $f.err $2
  locked && echo "LOCKED-AFTER" >> $f.lock
  pkill -f MacOS/FumiGPUI; sleep 1
}
for r in 1 2 3; do
  for sd in launch:none open:none type:bench200k type:bench1m scroll:bench1m idle:bench200k; do
    s=${sd%%:*}; d=${sd##*:}
    echo "--- r$r $s $d $(date +%H:%M:%S) load $(sysctl -n vm.loadavg)"
    one base $BASE $s $d $r; one fast $FAST $s $d $r
  done
done
rm -rf $B/lib; echo ALLDONE
