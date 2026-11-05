#!/bin/zsh
# Runs every scenario on every fixture, each in a fresh process. Needs the window
# in front the whole time; typing elsewhere meanwhile would show up as failures.
R=${0:A:h:h}; out=$R/results/matrix.txt; : > $out
for d in ${=FIXTURES:-10k 50k 200k 300k 500k 700k 1m}; do
  for s in ${=SCENARIOS:-idle reopen type table enter backspace caret fast scroll fling}; do
    echo "--- load: $(sysctl -n vm.loadavg)" >> $out
    $R/scripts/drive.sh $s $d 60 >> $out 2>&1
  done
done
echo DONE >> $out
