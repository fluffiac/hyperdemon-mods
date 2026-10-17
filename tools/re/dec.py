# usage: dec.py addr [addr...]  -> writes pc/<addr>.c via broker batch_decompile
import sys,json,socket,os
S=os.path.dirname(os.path.abspath(__file__))
os.makedirs(S+'/pc',exist_ok=True)
addrs=[a.lower() for a in sys.argv[1:]]
c=socket.create_connection(('127.0.0.1',47811)); c.sendall((json.dumps({'tool':'batch_decompile','args':{'addresses':addrs}})+'\n').encode())
buf=b''
while True:
    x=c.recv(1<<20)
    if not x: break
    buf+=x
res=json.loads(buf)
open(S+'/pc/_last.json','wb').write(buf)
def find_texts(o,acc):
    if isinstance(o,dict):
        if any(k in o for k in ('pseudo_code','pseudocode','code')) :
            acc.append(o)
        for v in o.values(): find_texts(v,acc)
    elif isinstance(o,list):
        for v in o: find_texts(v,acc)
acc=[]; find_texts(res,acc)
for it in acc:
    code=it.get('pseudo_code') or it.get('pseudocode') or it.get('code')
    a=it.get('address') or it.get('procedure') or it.get('selector') or 'x'
    if isinstance(code,str):
        fn=f"{S}/pc/{a}.c"; open(fn,'w',encoding='utf-8').write(code); print(a,len(code),'chars')
if not acc: print(str(res)[:1500])
