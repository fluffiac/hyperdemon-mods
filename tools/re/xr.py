import os
import sys,struct,bisect
EXE=os.environ.get("HD_GAME_EXE",r"C:\Program Files (x86)\Steam\steamapps\common\hyperdemon\hyperdemon.exe")
d=open(EXE,'rb').read()
IB=0x140000000; TEXT_RVA=0x1000; TEXT_RAW=0x400; TEXT_SZ=0x313000
text=d[TEXT_RAW:TEXT_RAW+TEXT_SZ]
# .pdata function starts
PD_RAW=0x3d5a00; PD_SZ=0x1b1f8
funcs=[]
for i in range(0,PD_SZ,12):
    b,e,u=struct.unpack_from('<III',d,PD_RAW+i)
    if b: funcs.append((b,e))
funcs.sort(); starts=[f[0] for f in funcs]
def func_of(rva):
    i=bisect.bisect_right(starts,rva)-1
    return funcs[i] if i>=0 and funcs[i][0]<=rva<funcs[i][1] else None
def find_str(s):
    b=s.encode(); res=[]; i=d.find(b)
    while i!=-1:
        res.append(i); i=d.find(b,i+1)
    return res
def raw2va(off):
    for va,ra,sz in [(0x314000,0x313400,0xbbe00),(0x3d0000,0x3cf200,0x6800),(0x1000,0x400,0x313000)]:
        if ra<=off<ra+sz: return off-ra+va
def refs(target_rva):
    out=[]
    # scan any 4-byte disp where insn_end + disp == target, for common rip-rel forms
    for i in range(len(text)-7):
        disp=struct.unpack_from('<i',text,i)[0]
        # assume instruction ends right after disp (no imm)
        if TEXT_RVA+i+4+disp==target_rva:
            # check modrm rip-rel
            m=text[i-1]
            if (m&0xC7)==0x05: out.append(TEXT_RVA+i-3 if text[i-3] in (0x48,0x4c) else TEXT_RVA+i-2)
    return out
if __name__=='__main__':
    for s in sys.argv[1:]:
        for off in find_str(s):
            if d[off-1]!=0 and not s.startswith('\0'): pass
            va=raw2va(off)
            if va is None: continue
            # back up to string start
            st=off
            while d[st-1]!=0: st-=1
            va=raw2va(st)
            rs=refs(va)
            print(f"{s!r} @ {IB+va:#x}: " + ", ".join(f"{IB+r:#x} (fn {IB+func_of(r)[0]:#x})" if func_of(r) else f"{IB+r:#x}" for r in rs))
