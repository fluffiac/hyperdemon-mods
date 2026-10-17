"""Offline: decrypt and print the run spawn table that World::SetMode(2) (FUN_1400e0620) loads."""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from xr import d

def raw(va):
    rva = va - 0x140000000
    for v, r, s in [(0x314000, 0x313400, 0xbbe00), (0x3d0000, 0x3cf200, 0x6800), (0x1000, 0x400, 0x313000)]:
        if v <= rva < v + s: return rva - v + r

TABLE, SIZE = 0x1403d5760, 0x354          # obfuscated copy in .data
K1, K2 = 0x14035d430, 0x14035d4b0         # 0x80-byte key tables in .rdata
# type -> (manager offset in *(world+0x130), variant), from FUN_1400de900
KINDS = {2: ('+0xc0', 0), 5: ('+0xc0', 1), 9: ('+0xc0', 2), 3: ('+0xc8 ENEMY 4', 0), 8: ('+0xc8 ENEMY 4', 1),
         10: ('+0x88 ENEMY 3', 0), 15: ('+0x88 ENEMY 3', 1), 11: ('+0x98 ENEMY 1', 0), 13: ('+0x98 ENEMY 1', 1),
         12: ('+0x80 ENEMY 2', 0), 14: ('+0x80 ENEMY 2', 1)}

def decrypt():
    t1, t2 = d[raw(K1):raw(K1) + 0x80], d[raw(K2):raw(K2) + 0x80]
    b = bytearray(d[raw(TABLE):raw(TABLE) + SIZE])
    for i in range(len(b)):
        b[i] ^= t2[((t1[(i // 4) & 0x7f] >> 1) + i) & 0x7f] ^ t1[i & 0x7f]
    return bytes(b)

if __name__ == '__main__':
    b = decrypt()
    w = lambda i: struct.unpack_from('<i', b, 4 * i)[0]
    f = lambda i: struct.unpack_from('<f', b, 4 * i)[0]
    print('version', w(0), 'header', [w(1)] + [round(f(i), 3) for i in range(2, 6)], '(read by manager +0x70 vslot 0xc0)')
    n = w(16); print('entries', n)
    wave, i = 1, 17
    for _ in range(n):
        typ, val = w(i), f(i + 1); i += 2
        if typ == -1:
            print(f'  -- end of wave {wave} (value {val:g})'); wave += 1
        else:
            print(f'  wave {wave}: type {typ:2d} {KINDS.get(typ, ("no spawn", 0))} value {val:g}')
