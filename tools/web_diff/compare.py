"""Compares LibreOrbit's web server with OpenOrbit's web.py on the same requests.

Each server runs in turn on the real ports (80, 8090, 843, 9339, 8082) with a fresh copy of a test save; the same
requests are sent to both, and the answers (status, headers the client reads, body), the log files and the written
web_settings.json are compared. Times (login.php serverInfo, log line times) and ephemeral ports are masked.

Usage (Python 3, standard library only; port 80 needs root on Linux):
  python3 -I compare.py <OpenOrbit checkout> <libreorbit executable> <LibreOrbit root>
"""
import http.client, json, os, re, shutil, socket, subprocess, sys, tempfile, time

OO, EXE, LO_ROOT = (os.path.abspath(a) for a in sys.argv[1:4])
SERVER = os.path.join(OO, 'server')
X = 'unknownsession0123456789'          # an extra-client key the (absent) game server does not know
HEADERS = ('Content-Type', 'X-Custom-Header', 'Access-Control-Allow-Origin', 'Access-Control-Allow-Methods',
           'Access-Control-Allow-Headers', 'Content-Length')

# (method, port, path, cookie, body)
REQUESTS = [
    ('GET', 8090, '/', None, None),
    ('GET', 80, '/unityApi/links.php', None, None),
    ('POST', 80, '/unityApi/login.php', None, b''),
    ('POST', 80, '/unityApi/login.php', 'PHPSESSID=localtest', b''),
    ('POST', 80, '/unityApi/login.php', 'PHPSESSID=' + X, b''),
    ('POST', 80, '/unityApi/setLang.php', None, b'lang=FR'),
    ('GET', 8090, '/', None, None),
    ('POST', 80, '/unityApi/login.php', 'other=1; PHPSESSID=localtest', b'x=1'),
    ('POST', 80, '/unityApi/setLang.php', None, b'lang=xyz'),
    ('POST', 80, '/unityApi/setLang.php', 'PHPSESSID=' + X, b'lang=pt_br'),
    ('POST', 80, '/unityApi/setLang.php', 'PHPSESSID=localtest', b'a=1&lang=+de+'),
    ('GET', 8090, '/?session=' + X, None, None),
    ('GET', 8090, '/?session=localtest', None, None),
    ('GET', 8090, '/?session=', None, None),
    ('GET', 80, '/', None, None),
    ('GET', 80, '/spacemap/xml/maps.php', None, None),
    ('GET', 80, '/unityApi/events/spaceSearch.php', None, None),
    ('GET', 80, '//unityApi/events/spaceSearch.php', None, None),
    ('GET', 80, '/unityApi/events/ssRewards.php', None, None),
    ('GET', 80, '/unityApi/guide/ggRewards.php', None, None),
    ('GET', 80, '/unityApi/instance.php', None, None),
    ('POST', 80, '/unityApi/agent.php', None, b'Unity_ChangeLanguagetr'),
    ('GET', 8090, '/info/forum', None, None),
    ('GET', 8090, '/info/help', None, None),
    ('GET', 8090, '/info/zzz', None, None),
    ('GET', 8090, '/indexInternal.es?action=internalWebGL', 'dosid=localtest', None),
    ('GET', 8090, '/indexInternal.es?action=internalWebGL', None, None),
    ('GET', 8090, '/indexInternal.es?action=internalWebGL', 'dosid=' + X, None),
    ('GET', 80, '/flashAPI/brNews.php', None, None),
    ('GET', 80, '/resources/resource_command_center.xml', None, None),
    ('GET', 80, '/resources/../../web.py', None, None),
    ('GET', 80, '/do_img/global/libreorbit_missing.png', None, None),
    ('GET', 80, '/do_img/global/libreorbit_missing.png', None, None),
    ('HEAD', 80, '/unityApi/links.php', None, None),
    ('OPTIONS', 80, '/unityApi/login.php', None, None),
    ('PUT', 80, '/unityApi/agent.php', None, b'x'),
    ('GET', 80, '/nonexistent?x=1', None, None),
    ('DELETE', 80, '/x', None, None),
]


def mask(text):
    text = re.sub(r'("?\\?"timeStamp\\?": )\d+', r'\1<t>', text)
    text = re.sub(r'(\\?"serverDate\\?": \\?")\d\d\.\d\d\.\d{4}', r'\1<date>', text)
    text = re.sub(r'(\\?"serverTime\\?": \\?")\d\d:\d\d', r'\1<time>', text)
    return text


def add_cdn_samples(root):
    """A file in each CDN folder of the copy (the request list uses fixed names)."""
    for rel in ('flashAPI/brNews.php', 'resources/resource_command_center.xml'):
        assert os.path.isfile(os.path.join(root, 'cdn', rel)), rel


def make_save(tmp):
    save = os.path.join(tmp, 'save')
    slot = os.path.join(save, 'slots', 'main')
    os.makedirs(slot)
    subprocess.run([sys.executable, '-I', os.path.join(SERVER, 'panel', 'profile_tool.py'), 'new', os.path.join(slot, 'profile.json'),
                    'Tester', '2'], check=True, capture_output=True)
    with open(os.path.join(save, 'panel.json'), 'w') as fh:
        json.dump({'active_save': 'main'}, fh)
    return save


def wait_port(port, proc, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        if proc.poll() is not None:
            raise SystemExit('server exited: %s' % proc.returncode)
        try:
            socket.create_connection(('127.0.0.1', port), 0.2).close()
            return
        except OSError:
            time.sleep(0.1)
    raise SystemExit('port %d did not open' % port)


def run(kind, base_save, world=None):
    tmp = tempfile.mkdtemp(prefix='lo_web_%s_' % kind)
    save = os.path.join(tmp, 'save')
    shutil.copytree(base_save, save)
    logs = os.path.join(tmp, 'logs')
    profile = os.path.join(save, 'slots', 'main', 'profile.json')
    env = dict(os.environ, OO_SAVE_DIR=save, LO_ROOT=LO_ROOT)
    env.pop('OO_WORLD', None)
    if world:
        env['OO_WORLD'] = world
    if kind == 'python':
        cmd = [sys.executable, '-I', os.path.join(SERVER, 'web.py'), logs, '--profile=%s' % profile]
    else:
        cmd = [EXE, 'web', logs, '--profile=%s' % profile]
    p = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    try:
        for port in (8090, 80, 843):
            wait_port(port, p)
        out = []
        for method, port, path, cookie, body in REQUESTS:
            c = http.client.HTTPConnection('127.0.0.1', port, timeout=20)
            h = {'Cookie': cookie} if cookie else {}
            c.request(method, path, body=body, headers=h)
            r = c.getresponse()
            data = r.read()
            c.close()
            rec = {'req': '%s :%d %s' % (method, port, path), 'status': r.status}
            if method != 'DELETE':
                # a 204 has no Content-Length in LibreOrbit (HTTP forbids it); web.py sends 0
                rec['headers'] = {k: r.getheader(k) for k in HEADERS if not (r.status == 204 and k == 'Content-Length')}
                rec['body'] = mask(data.decode('utf8', 'replace'))
            out.append(rec)
        for port, payload in ((843, b'<policy-file-request/>\x00'), (9339, b'abc')):
            s = socket.create_connection(('127.0.0.1', port))
            s.sendall(payload)
            time.sleep(0.3)
            s.close()
        time.sleep(0.5)
    finally:
        p.terminate()
        try:
            p.wait(5)
        except subprocess.TimeoutExpired:
            p.kill()
    files = {}
    for name in ('http.log', 'unknown.log', 'tcp.log'):
        try:
            text = open(os.path.join(logs, name), encoding='utf8').read()
        except OSError:
            text = ''
        lines = [mask(l[9:]) for l in text.splitlines()]                          # without "HH:MM:SS "
        lines = [re.sub(r"connect from \('127\.0\.0\.1', \d+\)", "connect from ('127.0.0.1', <port>)", l) for l in lines]
        lines = [re.sub(r'(CDN download failed: \S+) \(.*\)$', r'\1 (<error>)', l) for l in lines]
        files[name] = lines
    try:
        files['web_settings.json'] = open(os.path.join(save, 'slots', 'main', 'web_settings.json'), encoding='utf8').read()
    except OSError:
        files['web_settings.json'] = None
    shutil.rmtree(tmp, ignore_errors=True)
    return out, files


def compare(label, world=None):
    base = make_save(tempfile.mkdtemp(prefix='lo_web_base_'))
    py, pyf = run('python', base, world)
    rs, rsf = run('rust', base, world)
    diffs = 0
    for a, b in zip(py, rs):
        if a != b:
            diffs += 1
            print('DIFF %s' % a['req'])
            for k in a:
                if a.get(k) != b.get(k):
                    print('  %s\n    python: %r\n    rust:   %r' % (k, str(a.get(k))[:600], str(b.get(k))[:600]))
    for name in pyf:
        if pyf[name] != rsf[name]:
            diffs += 1
            print('DIFF file %s' % name)
            a, b = pyf[name], rsf[name]
            if isinstance(a, list):
                for i, (x, y) in enumerate(zip(a, b)):
                    if x != y:
                        print('  line %d\n    python: %s\n    rust:   %s' % (i + 1, x[:400], y[:400]))
                if len(a) != len(b):
                    print('  %d lines vs %d' % (len(a), len(b)))
            else:
                print('    python: %r\n    rust:   %r' % (a, b))
    print('%s: %d requests, %d differences' % (label, len(py), diffs))
    return diffs


def main():
    add_cdn_samples(LO_ROOT)
    n = compare('standard world')
    w = tempfile.mktemp(suffix='.json')
    with open(w, 'w') as fh:
        json.dump({'name': 'test', 'patches': {'gates.json': {
            '/gates/alpha/reward/uridium': 12345, '/gates/beta': {'$delete': True},
            '/gates/zzz': {'reward': {'ep': 7.9, 'booty': {'green_key': 2}}}, '/nowhere/x': 1}}}, fh)
    n += compare('World Editor world', w)
    sys.exit(1 if n else 0)


if __name__ == '__main__':
    main()
