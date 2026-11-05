#!/usr/bin/env python3 -I
"""Tabulates results/<scenario>-<doc>.log. Prints medians in ms."""
import re, os, sys
R = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'results')
docs = ['10k', '50k', '200k', '300k', '500k', '700k', '1m']
def text(s, d):
    p = f'{R}/{s}-{d}.log'
    return open(p).read() if os.path.exists(p) else ''
def f(s, d, name):
    m = re.search(re.escape(name) + r': n=(\d+) failed=(\d+) \| main-thread busy p50=([\d.]+) p95=([\d.]+) max=([\d.]+)', text(s, d))
    return f'{m[3]} / {m[4]} / {m[5]}' + (f' ({m[2]} failed)' if m[2] != '0' else '') if m else '-'
print('op: main-thread busy p50 / p95 / max')
for label, s, name in [('type', 'type', 'type'), ('table', 'table', 'table'), ('enter', 'enter', 'enter'), ('backspace', 'backspace', 'backspace'),
                       ('caret down', 'caret', 'caret-down'), ('caret right', 'caret', 'caret-right'), ('open again', 'reopen', 'open-in-running-window')]:
    print(label)
    for d in docs:
        print(f'  {d:5} {f(s, d, name)}')
for s in ['scroll', 'fling']:
    print(s)
    for d in docs:
        m = re.search(r'RESULT \w+: (.*)', text(s, d))
        print(f'  {d:5} {m[1] if m else "-"}')
print('memory')
for d in docs:
    idle = re.search(r'idle-12s: footprint=(\d+)', text('idle', d))
    fl = re.search(r'footprint=(\d+)MB peak=(\d+)', text('fling', d))
    print(f'  {d:5} settled {idle[1] if idle else "?"}MB, after fling {fl[1] if fl else "?"}MB, peak {fl[2] if fl else "?"}MB')
