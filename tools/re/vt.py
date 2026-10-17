# usage: vt.py <vtable addr> [count]  -> slot, target, first bytes
import struct,sys,os
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from xr import d
def raw(va):
    rva=va-0x140000000
    for v,r,s in [(0x314000,0x313400,0xbbe00),(0x3d0000,0x3cf200,0x6800),(0x1000,0x400,0x313000)]:
        if v<=rva<v+s: return rva-v+r
b=raw(int(sys.argv[1],16)); n=int(sys.argv[2],0) if len(sys.argv)>2 else 8
for i in range(n):
    p=struct.unpack_from('<Q',d,b+i*8)[0]
    r=raw(p)
    print(hex(i*8), hex(p), d[r:r+16].hex() if r else '')
