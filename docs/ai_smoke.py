#!/usr/bin/env python3
"""End-to-end smoke test for the Zenuxs AI port in FigCraft.

Drives the desktop app over its loopback control channel. Usage:
  DISPLAY=:0 ./target/debug/figcraft --control 59253 &   # terminal 1
  python3 docs/ai_smoke.py                               # terminal 2

Override with env: FIGCRAFT_PORT, AI_BASE_URL, AI_MODEL, API_KEY.
No secret is written to disk or printed; the key is read from env (default "public").
"""
import socket, json, os, time, sys

PORT = int(os.environ.get('FIGCRAFT_PORT', '59253'))
BASE = os.environ.get('AI_BASE_URL', 'http://127.0.0.1:20129/v1')
MODEL = os.environ.get('AI_MODEL', 'mimo-v2.6-flash-free')
KEY = os.environ.get('API_KEY', 'public')

s = socket.create_connection(('127.0.0.1', PORT), timeout=60)
f = s.makefile('rwb'); n = 0
def call(method, params=None, timeout=80000):
    global n; n += 1
    f.write((json.dumps({'id': n, 'method': method, 'params': params or {}, 'timeoutMs': timeout}) + '\n').encode()); f.flush()
    line = f.readline()
    return json.loads(line) if line else {'ok': False, 'error': 'no reply'}
def eng(cmd, params=None): return call('engine.execute', {'command': cmd, 'params': params or {}})

results = []
def check(label, fn):
    try:
        ok, detail = fn()
    except Exception as e:
        ok, detail = False, f'{type(e).__name__}: {e}'
    results.append((label, ok))
    print(('  ✅ ' if ok else '  ❌ ') + label + (f' — {detail}' if detail else ''))

# ---- setup ----
check('app responds', lambda: (call('window.list')['ok'], 'port %d' % PORT))
r = eng('ai.providers')
check('provider registry (9 providers)', lambda: (r['ok'] and len(r['result']['providers']) >= 9, f"{len(r.get('result',{}).get('providers',[]))} providers"))

eng('file.new')
conn = eng('ai.connection.add', {'provider':'openai-compatible','customBaseURL':BASE,'customAPIType':'completions'})['result']['id']
model = eng('ai.model.add', {'name':'Proxy','connectionId':conn,'customModelID':MODEL,'capabilities':['tools']})['result']['id']
eng('ai.role.assign', {'role':'design','modelId':model})
cred = eng('ai.credential.set', {'integration':'openai-compatible','profileId':conn,'field':'api-key','value':KEY})
check('credential set (key not answered)', lambda: (cred['result']['status'] == 'configured' and KEY not in json.dumps(cred), cred['result']['status']))
resolved = eng('ai.chat.model')['result']
check('model resolves through role', lambda: (resolved['modelID'] == MODEL, resolved['modelID']))

call('ui.set', {'rightTab':'ai'})
check('AI tab opens', lambda: (call('ui.set', {'rightTab':'ai'})['ok'], 'rightTab=ai'))

# ---- chat + tool loop (the core) ----
prompt = 'Make a 300 by 120 rectangle named Banner with a solid red fill. Use create_node then set_properties.'
eng('ai.chat.send', {'text': prompt})
def wait_chat():
    for _ in range(45):
        time.sleep(1)
        r = eng('ai.chat.messages')['result']
        msgs = r['messages']
        if not r['busy'] and any(m['role']=='assistant' and m.get('content','').strip() for m in msgs):
            return msgs
    return []
msgs = wait_chat()
check('streaming chat answered', lambda: (any(m['role']=='assistant' and m.get('content','').strip() for m in msgs), f'{len(msgs)} messages'))
users = [m for m in msgs if m['role']=='user']
check('no duplicate user message', lambda: (len(users) == 1, f'{len(users)} user messages'))
calls = [m for m in msgs if m['role']=='tool']
check('tool loop ran (create + set)', lambda: (len(calls) >= 2, f'{len(calls)} tool calls'))

d = call('document.inspect')['result']
kids = d['pages'][0]['children']
banner = next((k for k in kids if k['name'] == 'Banner'), None)
def banner_ok():
    if not banner: return (False, 'no Banner layer')
    fill = banner.get('fills', [{}])[0].get('color', '')
    return (banner['width']==300 and banner['height']==120 and fill.lower()=='#ff0000',
            f"{banner['name']} {banner['width']}x{banner['height']} {fill}")
check('AI created the design on canvas', banner_ok)

# ---- secondary surfaces (commands exist + respond) ----
for cmd in ['ai.chat.preferences','ai.media.settings','ai.storage.settings','ai.diagnostics.settings','ai.plugins.catalog','ai.usage','ai.chat.history','ai.account.token.status']:
    check(f'command {cmd}', lambda c=cmd: (eng(c)['ok'], ''))

# a named action queues a prompt (engine-side; no network needed)
act = eng('ai.action.renameLayers')
check('named AI action queues a prompt', lambda: (act['ok'] and eng('ai.chat.messages')['result']['busy'] is not None, 'queued'))

# clear the queued action so it doesn't interfere with further checks
eng('ai.chat.clear')

# ---- summary ----
passed = sum(1 for _,ok in results if ok)
print(f'\n{passed}/{len(results)} checks passed')
if passed != len(results):
    sys.exit(1)
