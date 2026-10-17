import os
import json,socket,re,struct,sys
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from xr import d,text,TEXT_RVA,TEXT_RAW,IB
def q(tool,args):
    c=socket.create_connection(('127.0.0.1',47811)); c.sendall((json.dumps({'tool':tool,'args':args})+'\n').encode())
    b=b''
    while True:
        x=c.recv(1<<20)
        if not x: break
        b+=x
    j=json.loads(b); return j.get('normalized_result',j)
def insns_from_rea(fn):
    r=q('read_function_instructions',{'procedure':hex(fn)})
    out=[]
    for s in r['instructions']:
        a,t=s.split(': ',1); out.append((int(a,16),t))
    return out
def va2off(va): return va-IB-TEXT_RVA+TEXT_RAW
def wild(insns):
    """returns list of (byte or None) for given instructions"""
    pat=[]
    for i,(a,t) in enumerate(insns):
        end=insns[i+1][0] if i+1<len(insns) else None
        if end is None: break
        raw=d[va2off(a):va2off(end)]
        mask=[True]*len(raw)
        for m in re.finditer(r'0x(14[0-9a-f]{7})',t):
            tgt=int(m.group(1),16)
            for k in range(len(raw)-3):
                if struct.unpack_from('<i',raw,k)[0]+end==tgt:
                    for j in range(k,k+4): mask[j]=False
        pat+= [b if mk else None for b,mk in zip(raw,mask)]
    return pat
def count(pat):
    # first literal run anchors search
    fixed=bytes(b for b in pat if b is not None)
    first=next(i for i,b in enumerate(pat) if b is not None)
    run=b''
    for b in pat[first:]:
        if b is None: break
        run+=bytes([b])
    n=0;hits=[]
    i=text.find(run)
    while i!=-1:
        st=i-first
        if st>=0 and all(b is None or text[st+k]==b for k,b in enumerate(pat)):
            n+=1;hits.append(st)
        i=text.find(run,i+1)
    return n,hits
def sig_for(fn,start_at=None,min_len=12):
    ins=insns_from_rea(fn)
    if start_at: ins=[x for x in ins if x[0]>=start_at]
    for k in range(2,len(ins)):
        p=wild(ins[:k+1])
        if len(p)>=min_len:
            n,h=count(p)
            if n==1 and h[0]+TEXT_RVA+IB==ins[0][0]:
                return ' '.join('??' if b is None else f'{b:02X}' for b in p)
    return None
def raw_sig(va,n):
    o=va2off(va); p=list(d[o:o+n]); c,h=count(p)
    return (' '.join(f'{b:02X}' for b in p), c)
if __name__=='__main__':
    for a in sys.argv[1:]:
        print(a, sig_for(int(a,16)))
