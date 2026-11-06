import re,sys,glob,os,statistics as st
d=sys.argv[1]; rows={}
def add(k,v): rows.setdefault(k,[]).append(v)
for f in sorted(glob.glob(d+'/*.log')):
    b=os.path.basename(f)[:-4]; build,sc,doc,run=b.split('-')
    if run=='0': continue
    for l in open(f):
        m=re.match(r'launch->(notesLoaded|settled) (\d+) ms.*footprint=(\d+)MB',l)
        if m and sc=='launch': add((build,'launch '+m[1]+' ms'),float(m[2])); add((build,'launch '+m[1]+' MB'),float(m[3]))
        m=re.match(r'search (\S+) hits=\d+ ([\d.]+) ms',l)
        if m: add((build,'search '+m[1]+' ms'),float(m[2]))
        m=re.match(r'idle cpu=([\d.]+)%',l)
        if m: add((build,'idle cpu %'),float(m[1]))
        m=re.match(r'(\S.*?)\s+busy n=(\d+)\s+p50=\s*([\d.]+) p95=\s*([\d.]+) max=\s*([\d.]+) sum=\s*([\d.]+) over16=(\d+)',l)
        if m:
            name=m[1].strip()
            t=re.search(r'\s+t=(\d+)ms',name)
            if t: name=name[:t.start()]; add((build,name+' t ms'),float(t[1]))
            if name.startswith('after-pause') or name=='launch' and sc!='launch': continue
            add((build,name+' busy p50'),float(m[3])); add((build,name+' busy p95'),float(m[4])); add((build,name+' busy max'),float(m[5])); add((build,name+' over16'),float(m[7]))
            mo=re.search(r'op n=\d+ p50=([\d.]+) p95=([\d.]+)',l)
            if mo: add((build,name+' op p50'),float(mo[1]))
keys=sorted({k for _,k in rows})
B=[b for b in ['base','fast','fastnr','fastnl'] if any(k[0]==b for k in rows)]
print(f"{'metric':42} "+' '.join(f"{b:>16}" for b in B))
for k in keys:
    def c(b):
        v=rows.get((b,k));
        return f"{st.median(v):8.2f} (n={len(v)})" if v else '-'
    print(f"{k:42} "+' '.join(f"{c(b):>16}" for b in B))
