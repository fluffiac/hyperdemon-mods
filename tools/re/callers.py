import os
import sys,struct,re
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from xr import text,TEXT_RVA,IB,func_of
targets={int(s,16)-IB:s for s in sys.argv[1:]}
res={s:[] for s in sys.argv[1:]}
for m in re.finditer(rb'[\xe8\xe9]',text):
    i=m.start()
    if i+5>len(text): break
    t=TEXT_RVA+i+5+struct.unpack_from('<i',text,i+1)[0]
    if t in targets:
        f=func_of(TEXT_RVA+i)
        res[targets[t]].append(f"{IB+TEXT_RVA+i:#x}{'' if text[i]==0xE8 else '[jmp]'}(fn {IB+f[0]:#x})" if f else hex(IB+TEXT_RVA+i))
for s,v in res.items(): print(s,'<-',', '.join(v))
