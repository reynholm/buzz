//! Bounded synthetic subprocess peer for background lifecycle regressions.
// The child exits only when the test creates a marker. This exercises actual
// process enumeration without scheduler-dependent sleep durations.
pub(super) const ADAPTER: &str = r#"
import json, os, subprocess, sys, threading, time
kimi = False
child = None
cancelled = threading.Event()
permission_received = threading.Event()
write_lock = threading.Lock()
def respond(req, result=None, error=None):
    reply = {'jsonrpc': '2.0', 'id': req['id']}
    reply['error' if error else 'result'] = error if error else result
    with write_lock:
        print(json.dumps(reply), flush=True)
def prompt(req):
    text = str(req['params']['prompt'])
    collecting = 'background child process' in text
    if collecting:
        open(os.path.join(os.path.dirname(sys.argv[1]), 'collection-started'), 'w').close()
        crash = os.path.join(os.path.dirname(sys.argv[1]), 'crash-collection')
        if kimi and os.path.exists(crash):
            os.remove(crash)
            print(json.dumps({'jsonrpc':'2.0','id':0,'method':'session/request_permission','params':{'sessionId':'original-session','toolCall':{'toolCallId':'0:collection-read','title':'Read','kind':'read'},'options':[{'kind':'allow_once','optionId':'approve_once'},{'kind':'reject_once','optionId':'reject'}]}}),flush=True)
            permission_received.wait(2)
            os._exit(17)
        while os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'block-collection')) and not cancelled.is_set():
            time.sleep(.01)
        if os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'fail-collection')):
            respond(req, error={'code': -32000, 'message': 'controlled collection failure'})
            return
    if collecting and cancelled.is_set():
        open(os.path.join(os.path.dirname(sys.argv[1]), 'cancel-received'), 'w').close()
        while os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'hold-cancel-ack')):
            time.sleep(.01)
    next_group = os.path.join(os.path.dirname(sys.argv[1]), 'next-group')
    if collecting and os.path.exists(next_group):
        os.remove(next_group)
        program = 'import os,sys,time\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)'
        subprocess.Popen([sys.executable, '-c', program, os.path.join(os.path.dirname(sys.argv[1]), 'finish-next')])
    respond(req, {'stopReason': 'cancelled' if cancelled.is_set() else 'end_turn'})
for line in sys.stdin:
    req = json.loads(line)
    with open(sys.argv[2], 'a') as log:
        log.write(line)
    if 'method' not in req:
        if req.get('id') == 0: permission_received.set()
        continue
    if req['method'] == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
        if kimi: result['agentInfo'] = {'name':'Kimi Code CLI','version':'synthetic-background-crash'}
        respond(req, result)
    elif req['method'] == 'session/new':
        respond(req, {'sessionId': 'original-session'})
    elif req['method'] == 'session/prompt':
        if child is None and (not kimi or 'background child process' not in str(req['params']['prompt'])):
            with open(os.path.join(os.path.dirname(sys.argv[1]), 'background-launches'),'a') as log: log.write('launch\n')
            program = 'import os,sys,time\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)'
            child = subprocess.Popen([sys.executable, '-c', program, sys.argv[1]])
        cancelled.clear()
        threading.Thread(target=prompt, args=(req,)).start()
    elif req['method'] == 'session/cancel':
        cancelled.set()
    elif 'id' in req:
        respond(req, {})
"#;

pub(super) fn background_adapter(kimi: bool) -> String {
    if kimi {
        ADAPTER.replace("kimi = False", "kimi = True")
    } else {
        ADAPTER.into()
    }
}
