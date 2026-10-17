import subprocess,json,sys,os,threading,queue
class Rea:
    def __init__(self):
        env=dict(os.environ)
        if not os.path.isdir(env.get('GHIDRA_INSTALL_DIR','')): sys.exit('set GHIDRA_INSTALL_DIR to your Ghidra install folder (the one containing support\\analyzeHeadless.bat)')
        env.setdefault('REA_GHIDRA_STARTUP_TIMEOUT_MS','1200000'); env.setdefault('GHIDRA_HEADLESS_MAXMEM','6G')
        self.p=subprocess.Popen(['cmd.exe','/d','/c','rea','mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open(os.devnull,'w'),env=env)
        self.q=queue.Queue(); self.id=0
        threading.Thread(target=self._rd,daemon=True).start()
        self.req('initialize',{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"hd","version":"1"}})
        self.note('notifications/initialized')
    def _rd(self):
        for line in self.p.stdout:
            try: self.q.put(json.loads(line))
            except Exception: pass
    def note(self,m,p=None):
        self.p.stdin.write((json.dumps({"jsonrpc":"2.0","method":m,**({"params":p} if p else {})})+"\n").encode()); self.p.stdin.flush()
    def req(self,m,p,timeout=1800):
        self.id+=1; i=self.id
        self.p.stdin.write((json.dumps({"jsonrpc":"2.0","id":i,"method":m,"params":p})+"\n").encode()); self.p.stdin.flush()
        while True:
            r=self.q.get(timeout=timeout)
            if r.get('id')==i: return r
    def call(self,name,args,timeout=1800):
        r=self.req('tools/call',{"name":name,"arguments":args},timeout)
        if 'error' in r: return {'error':r['error']}
        res=r['result']
        if res.get('structuredContent') is not None: return res['structuredContent']
        txt=''.join(c.get('text','') for c in res.get('content',[]))
        try: return json.loads(txt)
        except Exception: return txt
