import os
import socket,json,sys
tool=sys.argv[1]; args=json.loads(sys.argv[2]) if len(sys.argv)>2 else {}
c=socket.create_connection(('127.0.0.1',47811)); c.sendall((json.dumps({'tool':tool,'args':args})+'\n').encode())
buf=b''
while True:
    x=c.recv(1<<20)
    if not x: break
    buf+=x
out=sys.argv[3] if len(sys.argv)>3 else None
if out: open(out,'wb').write(buf); print('wrote',len(buf))
else:
    j=json.loads(buf); n=j.get("normalized_result",j) if isinstance(j,dict) else j
    s=n if isinstance(n,str) else json.dumps(n); print(s[:int(sys.argv[4]) if len(sys.argv)>4 else 6000])
