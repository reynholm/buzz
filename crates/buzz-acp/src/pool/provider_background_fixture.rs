//! In-process task fixture shared by host lifecycle and runtime delivery journeys.
pub(in crate::pool) const ADAPTER: &str = r#"
import json,os,sys,threading,time
root=sys.argv[1] if len(sys.argv)>1 else os.path.dirname(os.environ['LOG'])
lock=threading.RLock()
saved=os.path.join(root,'provider-state')
stored=json.load(open(saved)) if os.path.exists(saved) else {}
identity=stored.get('identity')
tasks=stored.get('tasks',{})
def send(v):
    with lock: print(json.dumps(v),flush=True)
def emit(t):
    with lock:
        with open(saved+'.tmp','w') as out: json.dump({'identity':identity,'tasks':tasks},out)
        os.replace(saved+'.tmp',saved)
        send({'jsonrpc':'2.0','method':'_buzz/background_tasks/update','params':dict(identity,**t)})
def refresh():
    with lock:
        for key,t in list(tasks.items()):
            if t['state']=='running' and os.path.exists(os.path.join(root,key)):
                t.update(state='completed_pending_collection',sequence=2)
                emit(t)
def finish():
    while True:
        refresh()
        time.sleep(.01)
threading.Thread(target=finish,daemon=True).start()
for line in sys.stdin:
    r=json.loads(line)
    with open(os.path.join(root,'requests'),'a') as log: log.write(line)
    m=r.get('method'); result={}
    if m=='initialize':
        result={'protocolVersion':2,'agentInfo':{'name':'qwen-code','version':'0.26.7'},'agentCapabilities':{'sessionCapabilities':{'resume':{}}},'_meta':{'buzz':{'backgroundTasks':{'version':1}},'steering':{'supported':os.path.exists(os.path.join(root,'native-steering'))}}}
    elif m in ('session/new','session/resume'): result={'sessionId':'original-session'}
    elif m=='_buzz/background_tasks/snapshot':
        open(os.path.join(root,'snapshot-started'),'w').close()
        while os.path.exists(os.path.join(root,'block-snapshot')): time.sleep(.01)
        if os.path.exists(os.path.join(root,'fail-snapshot')):
            send({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32000,'message':'temporary snapshot failure'}})
            continue
        refresh()
        result=dict(r['params'],tasks=list(tasks.values()))
    elif m=='_buzz/background_tasks/cancel':
        send({'jsonrpc':'2.0','id':'cancel-permission','method':'session/request_permission','params':{'sessionId':'original-session','toolCall':{'title':'late action'},'options':[{'optionId':'yes','name':'Allow','kind':'allow_once'}]}})
        permission=json.loads(sys.stdin.readline())
        with open(os.path.join(root,'cancel-permission'),'w') as out: json.dump(permission,out)
        result=dict(r['params'],state='cancelled',durable=True)
        with open(os.path.join(root,'cancel-receipt'),'w') as out: json.dump(result,out)
        if os.path.exists(os.path.join(root,'invalid-cancel-ack')): result['cancel_id']='wrong'
    elif m=='session/prompt':
        meta=r['params'].get('_meta',{}).get('buzz',{}).get('backgroundTasks',{})
        if identity is None or meta.get('identity',identity)!=identity:
            with lock:
                tasks={}
                identity=meta.get('identity',{'session_id':'original-session','work_id':'00000000-0000-4000-8000-000000000001','generation':1})
                with open(os.path.join(root,'launches'),'a') as log: log.write('launch\n')
                for i in range(3):
                    t={'task_id':'child-'+str(i),'state':'running','sequence':1}; tasks[t['task_id']]=t; emit(t)
            open(os.path.join(root,'prompt-started'),'w').close()
            while os.path.exists(os.path.join(root,'block-prompt')): time.sleep(.01)
        elif meta.get('collect'):
            open(os.path.join(root,'collection-started'),'w').close()
            while os.path.exists(os.path.join(root,'block-collection')): time.sleep(.01)
            with open(os.path.join(root,'collections'),'a') as log: log.write(json.dumps(meta['collect'])+'\n')
            if not os.path.exists(os.path.join(root,'omit-ack')):
                with lock:
                    for t in meta['collect']:
                        t=dict(t,state='collected',completion_sequence=t['sequence'],sequence=t['sequence']+1)
                        tasks[t['task_id']]=t; emit(t)
        if tasks and all(t['state']=='collected' for t in tasks.values()):
            open(os.path.join(root,'results-acked'),'w').close()
            if not meta.get('collect'): open(os.path.join(root,'parent-resumed'),'w').close()
            while os.path.exists(os.path.join(root,'block-final')): time.sleep(.01)
            send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'original-session','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'All three existing results were collected.'}}}})
        result={'stopReason':'end_turn'}
    if 'id' in r: send({'jsonrpc':'2.0','id':r['id'],'result':result})
"#;
