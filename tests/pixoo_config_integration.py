#!/usr/bin/env python3
"""Exercise persistence and live config against an isolated, simulated server.
Run after cargo build --offline --locked --bin cts-departures.
"""
import copy, json, os, pathlib, socket, subprocess, tempfile, time, tomllib, urllib.error, urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]
BINARY=ROOT/'target/debug/cts-departures'
with tempfile.TemporaryDirectory(prefix='cts-pixoo-config-') as tmp:
    folder=pathlib.Path(tmp)
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    config=folder/'config.toml'
    config.write_text(f'''# Preserve this comment
cts_api_token = "fictional-only"
cts_monitoring_ref = ["298B", "298A"]
cts_polling_interval_minutes = 2
cts_simulation = true
listen_addr = "127.0.0.1:{port}"
pixoo64_enabled = true
pixoo64_simulation = true
jour_j_enabled = true
jour_j_events = []
''')
    config.chmod(0o600)
    base=f'http://127.0.0.1:{port}'
    def request(path='/api/pixoo64/config',body=None,headers=None):
        req=urllib.request.Request(base+path,None if body is None else json.dumps(body).encode(),{'Content-Type':'application/json',**(headers or {})})
        try:
            with urllib.request.urlopen(req,timeout=3) as r:return r.status,r.read()
        except urllib.error.HTTPError as e:return e.code,e.read()
    def start():
        p=subprocess.Popen([str(BINARY),str(config)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        for _ in range(50):
            assert p.poll() is None,'Test server exited'
            try:
                if request('/api/status')[0]==200:return p
            except OSError:pass
            time.sleep(.1)
        p.terminate();raise AssertionError('Test server did not start')
    p=start()
    try:
        initial=json.loads(request()[1]);assert [s['seconds'] for s in initial['settings']['steps']]==[12,6,1,1,12,6,1,1]
        assert request('/api/config',{'monitoring_refs':['298B']})[0]==200
        assert [s['seconds'] for s in json.loads(request()[1])['settings']['steps']]==[12,1,1,12,1,1]
        assert request('/api/config',{'monitoring_refs':['298B','298A']})[0]==200
        expected={'lines_per_screen':2,'steps':[{'screen':'miniature','platform':'298A','seconds':23},{'screen':'station','seconds':7},{'screen':'events','seconds':3}]}
        assert request(body=expected,headers={'Origin':base})[0]==200
        assert json.loads(request()[1])['settings']==expected
        saved=config.read_bytes();stored=tomllib.loads(saved.decode());assert stored['pixoo64_sequence']==expected
        assert stored['cts_api_token']=='fictional-only' and stored['cts_monitoring_ref']==['298B','298A']
        assert b'# Preserve this comment' in saved and config.stat().st_mode&0o777==0o600
        invalid=[]
        for seconds in [0,301,-1,1.5]:
            v=copy.deepcopy(expected);v['steps'][0]['seconds']=seconds;invalid.append(v)
        v=copy.deepcopy(expected);v['steps'][0]['platform']='unknown';invalid.append(v)
        v=copy.deepcopy(expected);v['steps'][0]['screen']='unknown';invalid.append(v)
        invalid += [{'lines_per_screen':4,'steps':[]},{'lines_per_screen':4,'steps':[{'screen':'birthdays','seconds':2}]},{'lines_per_screen':4,'steps':[{'screen':'station','seconds':1}]*65}]
        for v in invalid:assert request(body=v)[0] in (400,422)
        assert request(headers={'X-CTS-External':'1'})[0]==403
        assert request(body=expected,headers={'X-CTS-External':'1'})[0]==403
        assert request(body=expected,headers={'Origin':'https://unrelated.invalid'})[0]==403
        assert json.loads(request()[1])['settings']==expected and config.read_bytes()==saved
        # A failed write must not switch the running playlist.
        changed=copy.deepcopy(expected);changed['steps'][0]['seconds']=42
        config.chmod(0o400)
        try:assert request(body=changed)[0]==500
        finally:config.chmod(0o600)
        assert json.loads(request()[1])['settings']==expected and config.read_bytes()==saved
        # Other configuration writers must preserve the device playlist.
        assert request('/api/jour-j',{'events':[],'birthday_days_ahead':9})[0]==200
        assert request('/api/config',{'monitoring_refs':['298B','298A']})[0]==200
        assert tomllib.loads(config.read_text())['pixoo64_sequence']==expected
        # Arrêt controls both the available models and the effective saved cycle.
        assert request('/api/config',{'monitoring_refs':['298B']})[0]==200
        single=json.loads(request()[1])
        assert [p['ref'] for p in single['platforms']]==['298B']
        assert single['settings']['steps']==expected['steps'][1:]
        for preset in single['presets'].values():
            assert all(s.get('platform','298B')=='298B' for s in preset['steps'])
        assert request(body=expected)[0]==400
        # Replacing every platform retains both designs with their first durations.
        assert request(body=single['presets']['alternating'])[0]==200
        assert request('/api/config',{'monitoring_refs':['new-platform']})[0]==200
        replaced=json.loads(request()[1])['settings']
        assert [s['seconds'] for s in replaced['steps']]==[12,1,1,12,1,1]
        assert all('platform' not in s for s in replaced['steps'])
        assert request('/api/config',{'monitoring_refs':['298B','298A']})[0]==200
        assert request(body=expected)[0]==200
        p.terminate();p.wait(timeout=10);p=start()
        assert json.loads(request()[1])['settings']==expected
        for _ in range(20):
            status,png=request('/api/pixoo64/preview')
            if status==200 and png.startswith(b'\x89PNG'):break
            time.sleep(.1)
        else:raise AssertionError('No preview after restart')
        p.terminate();p.wait(timeout=10)
        config.write_text(config.read_text().replace('pixoo64_enabled = true','pixoo64_enabled = false'))
        p=start()
        assert request()[0]==404 and request(body=expected)[0]==404
        assert request('/api/status')[0]==200
        # Cold start outside service hours: no departures or CTS request needed.
        p.terminate();p.wait(timeout=10)
        config.write_text(config.read_text().replace('pixoo64_enabled = false','pixoo64_enabled = true') + '\ncts_always_query = false\ncts_query_intervals = "0 0 31 2 *"\npixoo64_platform_labels = { "other-quai" = { name = "Autre arrêt", directions = ["A → Nord"] } }\n')
        p=start()
        offline=json.loads(request()[1])
        assert [v['name'] for v in offline['platforms']]==['Jean Jaurès','Jean Jaurès']
        assert offline['platforms'][0]['directions']==['C → Gare Centrale','D → Poteries']
        assert 'Neuhof' in offline['platforms'][1]['directions'][0]
        assert 'Kehl' in offline['platforms'][1]['directions'][1]
        assert request('/api/config',{'monitoring_refs':['other-quai']})[0]==200
        other=json.loads(request()[1])
        assert other['platforms']==[{'ref':'other-quai','name':'Autre arrêt','directions':['A → Nord']}]
        assert request(body=other['settings'])[0]==200
        p.terminate();p.wait(timeout=10);p=start()
        assert json.loads(request()[1])['platforms']==other['platforms']
        print(json.dumps({'offline_names_and_directions':'OK','disabled_device_rejected' :'OK','single_platform_and_replacement':'OK','api_validation':'OK','external_and_cross_origin_rejected':'OK','write_failure_preserves_live_settings':'OK','comments_and_permissions_preserved':'OK','other_settings_preserved':'OK','restart_persistence':'OK','preview_after_restart':'OK'}))
    finally:
        p.terminate();p.wait(timeout=10)
