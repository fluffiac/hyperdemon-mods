import os
# Holds one REA MCP session (one Ghidra import) and serves tool calls on 127.0.0.1:47811
import socket,json,sys,time
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from mcp import Rea
r=Rea()
t=time.time()
print('open',json.dumps(r.call('open_binary',{"path":os.environ.get("HD_GAME_EXE",r"C:\Program Files (x86)\Steam\steamapps\common\hyperdemon\hyperdemon.exe")}))[:400],flush=True)
print('overview',str(r.call('binary_overview',{}))[:300],'secs',time.time()-t,flush=True)
s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',47811)); s.listen(4)
print('READY',flush=True)
while True:
    c,_=s.accept(); buf=b''
    while not buf.endswith(b'\n'):
        x=c.recv(65536)
        if not x: break
        buf+=x
    try:
        q=json.loads(buf); res=r.call(q['tool'],q.get('args',{}))
    except Exception as e: res={'broker_error':repr(e)}
    c.sendall(json.dumps(res).encode()); c.close()
