"""Compares LibreOrbit's game server with OpenOrbit's game.py: the same scripted client (lo-probe) plays the same steps on
each, with a fresh copy of the same save; the messages received and gs.log are compared (times and ports masked).

Usage (Python 3, standard library only):
  python3 -I compare.py <OpenOrbit checkout> <libreorbit executable> <lo-probe executable> <LibreOrbit root> [steps]
Steps: lo-probe's (default "version,keepalive,version"). LibreOrbit is not expected to match beyond what is ported.
"""
import collections, difflib, json, os, re, shutil, socket, subprocess, sys, tempfile, time

OO, EXE, PROBE, LO_ROOT = (os.path.abspath(a) for a in sys.argv[1:5])
STEPS = sys.argv[5] if len(sys.argv) > 5 else 'version,keepalive,version'
SERVER = os.path.join(OO, 'server')
STARTUP = ('settings:', 'world:', 'chat listening', 'gs listening')


def make_save(tmp):
    save = os.path.join(tmp, 'save')
    slot = os.path.join(save, 'slots', 'main')
    os.makedirs(slot)
    subprocess.run([sys.executable, '-I', os.path.join(SERVER, 'panel', 'profile_tool.py'), 'new', os.path.join(slot, 'profile.json'),
                    'Tester', '1'], check=True, capture_output=True)
    with open(os.path.join(save, 'panel.json'), 'w') as fh:
        json.dump({'active_save': 'main'}, fh)
    return save


def wait_port(port, proc, timeout=30):
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


def run(kind, base):
    tmp = tempfile.mkdtemp(prefix='lo_game_%s_' % kind)
    save = os.path.join(tmp, 'save')
    shutil.copytree(base, save)
    logs = os.path.join(tmp, 'logs')
    os.makedirs(logs)
    profile = os.path.join(save, 'slots', 'main', 'profile.json')
    env = dict(os.environ, OO_SAVE_DIR=save, LO_ROOT=LO_ROOT)
    env.pop('OO_WORLD', None)
    if kind == 'python':
        cmd = [sys.executable, '-I', os.path.join(SERVER, 'game.py'), logs, '--profile=%s' % profile]
    else:
        cmd = [EXE, 'game', logs, '--profile=%s' % profile]
    p = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    out = os.path.join(tmp, 'probe.jsonl')
    try:
        wait_port(8081, p)
        subprocess.run([PROBE, '127.0.0.1:8081', out, '--steps=' + STEPS, '--spec=' + os.path.join(LO_ROOT, 'data', 'protocol_read.json')],
                       check=True, timeout=300)
        time.sleep(0.5)
    finally:
        p.terminate()
        try:
            p.wait(5)
        except subprocess.TimeoutExpired:
            p.kill()
    msgs = [json.loads(l) for l in open(out, encoding='utf8')]
    lines = []
    for l in open(os.path.join(logs, 'gs.log'), encoding='utf8').read().splitlines():
        l = l[9:] if re.match(r'\d\d:\d\d:\d\d ', l) else l
        if l.startswith(STARTUP):
            continue
        lines.append(re.sub(r"\('127\.0\.0\.1', \d+\)", "('127.0.0.1', <port>)", l))
    shutil.rmtree(tmp, ignore_errors=True)
    return msgs, lines


def main():
    base = make_save(tempfile.mkdtemp(prefix='lo_game_base_'))
    py, pyl = run('python', base)
    rs, rsl = run('rust', base)
    # messages aligned (difflib): a missing or extra message counts once instead of shifting everything after it
    a = [json.dumps(m, sort_keys=True) for m in py]
    b = [json.dumps(m, sort_keys=True) for m in rs]
    diffs, shown = 0, 0
    missing, extra, changed = collections.Counter(), collections.Counter(), collections.Counter()
    for op, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if op == 'equal':
            continue
        for k in range(max(i2 - i1, j2 - j1)):
            x = py[i1 + k] if i1 + k < i2 else None
            y = rs[j1 + k] if j1 + k < j2 else None
            diffs += 1
            if x and y and x.get('name') == y.get('name'):
                changed[x['name']] += 1
            else:
                if x:
                    missing[x['name']] += 1
                if y:
                    extra[y['name']] += 1
            if shown < 20:
                shown += 1
                print('DIFF message %s/%s\n  python: %s\n  rust:   %s' % (i1 + k + 1, j1 + k + 1, json.dumps(x)[:300], json.dumps(y)[:300]))
    for i in range(max(len(pyl), len(rsl))):
        x = pyl[i] if i < len(pyl) else None
        y = rsl[i] if i < len(rsl) else None
        if x != y:
            diffs += 1
            if shown < 40:
                shown += 1
                print('DIFF gs.log line %d\n  python: %s\n  rust:   %s' % (i + 1, x, y))
    for label, c in (('different', changed), ('only in python', missing), ('only in rust', extra)):
        if c:
            print('%s: %s' % (label, ', '.join('%s %d' % kv for kv in c.most_common())))
    print('steps %s: %d messages (python) / %d (rust), %d log lines, %d differences' % (STEPS, len(py), len(rs), len(pyl), diffs))
    sys.exit(1 if diffs else 0)


if __name__ == '__main__':
    main()
