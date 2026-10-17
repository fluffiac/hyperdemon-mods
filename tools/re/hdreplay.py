"""Parser for HyperDemon .hdreplay / .hdclip files (layout from FUN_140105cf0 + FUN_1401062f0)."""
import os, struct, sys, zlib

class R:
    def __init__(s, b): s.b, s.p = b, 0
    def take(s, n):
        v = s.b[s.p:s.p+n]; s.p += n; return v
    def u8(s): return s.take(1)[0]
    def i16(s): return struct.unpack('<h', s.take(2))[0]
    def u32(s): return struct.unpack('<I', s.take(4))[0]
    def i32(s): return struct.unpack('<i', s.take(4))[0]
    def u64(s): return struct.unpack('<Q', s.take(8))[0]

def parse(b):
    r = R(b); h = {}
    magic = r.take(6)
    if magic[:5] != b'hdrpl': raise ValueError(f'bad magic {magic!r}')
    h['version'] = v = r.u8()
    if v <= 3: raise ValueError('version <= 3 unsupported')
    if v > 7: h['v8_u32'] = r.u32()
    h['has_clip'] = r.u8() != 0
    if h['has_clip']:
        h['clip_start'] = r.i32()      # H+0x2c -> Game+0x70 obj +0x320
        h['clip_u64'] = r.u64()        # H+0x20 -> Game+0x70 obj +0x324
    h['h30'] = r.u32()                 # -> replay+0x194
    h['h0c'] = r.u32()                 # -> Game+0x2e0
    h['h10'] = r.u32()                 # -> Game+0x2dc
    h['mode'] = r.u8()                 # H+0x14 -> Game+0x2bc (arena/mode index)
    if v > 7:
        h['h08'] = r.u32()             # -> Game+0x2d8
        h['h04'] = r.u32()
        h['h18'] = r.i32()
    h['h28'] = r.u32()                 # -> replay+0x1fc
    h['h34'] = r.u32()
    h['h38'] = r.u32()
    n = r.i16(); h['list40'] = [r.u32() for _ in range(n)]
    n = r.i32(); h['list2f8'] = [r.u32() for _ in range(n)]
    h['header_size'] = r.p
    body = b[r.p:]
    try:
        h['body'] = zlib.decompress(body); h['body_codec'] = 'zlib'
    except zlib.error:
        try:
            h['body'] = zlib.decompress(body, -15); h['body_codec'] = 'raw-deflate'
        except zlib.error as e:
            h['body'] = None; h['body_codec'] = f'unknown ({e}); first bytes {body[:8].hex()}'
    return h

if __name__ == '__main__':
    for f in sys.argv[1:]:
        h = parse(open(f, 'rb').read())
        body = h.pop('body')
        h['list40'] = h['list40'][:8]; h['list2f8'] = h['list2f8'][:8]
        print(os.path.basename(f), h, 'body', None if body is None else len(body))
