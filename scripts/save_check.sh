#!/bin/zsh
# Round trip on disk: a copy is opened, edited with real keys, undone, saved with
# Cmd-S, and must equal the original byte for byte. The fixture itself is untouched.
R=${0:A:h:h}; cd $R/fixtures
for d in ${=@:-tricky visual 10k 200k 1m}; do
  t=_save_$d; cp $d.md $t.md; before=$(shasum -a 256 $t.md | cut -c1-16)
  sleep 1; touch -t 200001010000 $t.md; $R/scripts/drive.sh save $t 25 | grep -E 'CHECK|failed' | cut -c1-120
  after=$(shasum -a 256 $t.md | cut -c1-16); m=$(stat -f %Sm -t %Y $t.md)
  cmp $d.md $t.md && echo "$d: identical on disk ($before), file rewritten: $([ $m != 2000 ] && echo yes || echo NO)"
  rm -f $t.md
done
