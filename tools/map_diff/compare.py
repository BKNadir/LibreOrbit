"""Compares LibreOrbit's map links and layout (crates/lo-game/src/map_links.rs, map_layout.rs) with OpenOrbit's map_links.py
and map_layout.py.

Both answer the same queries (oracle.py for Python, the map_query example for Rust): the tables read from the client's XML
files (names, levels, scales, every portal with its fields), every map's properties for each company (level required, X-8
home, portals in order, JumpgateCreateCommand, hints), the worlds of the maps not in the template (map_world, for each
company, with and without the template), the arrival point for every pair of maps, synthetic portal hints, station hints,
the layout (obstacles, zones, safe zones, roaming waypoints by A*) and the random draws (free spots, arrival, mine fields)
with the same seed on both sides (lo_core::pyrandom). Scenarios: standard, the panel's no-level-limit setting, a World
Editor world that changes map_layout.json.

Usage: python3 -I compare.py <OpenOrbit checkout> [--keep <dir>]     Exit status 1 if anything differs.
"""
import argparse, json, os, random, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
# answers whose numbers OpenOrbit keeps as read (int or float) while LibreOrbit computes in floats: compared by value
LENIENT = ('/walls', '/safe', '/structure', '/peace', 'segments', 'probe')
WORLD = {'name': 'map diff', 'patches': {'map_layout.json': {
    '/maps/92/portals/990006': [17000, 11000],
    '/maps/1/center': [9000, 7000],
    '/maps/4': {'$delete': True},
    '/maps/5/obstacles': [['rock1', 9000, 5000, 11000, 6000, 1], ['poly', 2000, 2000, 2600, 2400, 2, [2000, 2000, 2600, 2000, 2300, 2400]]],
    '/maps/5/safe': [[10000, 9000, 700]],
    '/maps/5/structure_asset': [12000, 9000],
    '/mine/count': 7,
}}}


def run(cmd, env, out):
    with open(out, 'w') as fh:
        r = subprocess.run(cmd, env=env, stdout=fh, stderr=subprocess.PIPE, text=True)
    if r.returncode:
        raise SystemExit('%s failed:\n%s' % (cmd[0], r.stderr))
    return json.load(open(out), parse_float=lambda s: ('float', float(s)), parse_int=lambda s: ('int', int(s)))


def diff(x, y, path, out, lenient=False):
    lenient = lenient or any(k in path for k in LENIENT) or re.match(r'random@\d+\[5\]', path) is not None   # mine_field_of
    if lenient and isinstance(x, tuple) and isinstance(y, tuple):
        if x[1] != y[1]:
            out.append((path, x, y))
    elif type(x) is not type(y):
        out.append((path, x, y))
    elif isinstance(x, dict):
        for k in list(x) + [k for k in y if k not in x]:
            if k not in x or k not in y:
                out.append((path + '/' + k, x.get(k, '<missing>'), y.get(k, '<missing>')))
            else:
                diff(x[k], y[k], path + '/' + k, out, lenient)
        if set(x) == set(y) and list(x) != list(y):
            out.append((path, 'key order', list(x)[:8], list(y)[:8]))
    elif isinstance(x, list):
        if len(x) != len(y):
            out.append((path, 'length %d' % len(x), 'length %d' % len(y)))
        for i, (p, q) in enumerate(zip(x, y)):
            diff(p, q, '%s[%d]' % (path, i), out, lenient)
    elif x != y:
        out.append((path, x, y))


def leaves(x):
    if isinstance(x, dict):
        return sum(leaves(v) for v in x.values())
    if isinstance(x, list):
        return sum(leaves(v) for v in x) or 1
    return 1


def plain(x):
    """The tagged numbers back to plain values."""
    if isinstance(x, tuple):
        return x[1]
    if isinstance(x, dict):
        return {k: plain(v) for k, v in x.items()}
    if isinstance(x, list):
        return [plain(v) for v in x]
    return x


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('openorbit')
    ap.add_argument('--keep')
    a = ap.parse_args()
    oo = os.path.abspath(a.openorbit)
    subprocess.run(['cargo', 'build', '--quiet', '--release', '-p', 'lo-game', '--example', 'map_query'], cwd=ROOT, check=True)
    exe = os.path.join(ROOT, 'target', 'release', 'examples', 'map_query')
    tmp = a.keep or tempfile.mkdtemp(prefix='map_diff_')
    os.makedirs(tmp, exist_ok=True)
    settings = os.path.join(tmp, 'settings.json')
    json.dump({'no_level_limit': True}, open(settings, 'w'))
    world = os.path.join(tmp, 'world.json')
    json.dump(WORLD, open(world, 'w'))
    total = bad = 0
    try:
        for name, sc, w in (('standard', {}, None), ('no level limit', {'settings': settings}, None), ('world', {}, world)):
            env = {k: v for k, v in os.environ.items() if k != 'OO_WORLD'}
            if w:
                env['OO_WORLD'] = w
            scf = os.path.join(tmp, 'scenario.json')
            json.dump(sc, open(scf, 'w'))
            qf = os.path.join(tmp, 'queries.json')
            json.dump([{'op': 'tables'}], open(qf, 'w'))
            tables = plain(run([sys.executable, '-I', os.path.join(HERE, 'oracle.py'), oo, scf, qf], env,
                               os.path.join(tmp, 'tables.json'))[0])
            maps = sorted(int(k) for k in tables['names']) + [0, 666, 999]
            keys = sorted({g['key'] for gs in tables['gates'].values() for g in gs} | {990006, 1, 2, 3})
            rnd = random.Random(name)
            qs = [{'op': 'tables'}]
            qs += [{'op': 'map', 'map': m, 'keys': keys} for m in maps]
            qs += [{'op': 'world', 'map': m} for m in maps]
            qs += [{'op': 'arrival', 'map': d, 'src': s} for s in maps for d in maps]
            synthetic = [{'tooltip': 'ttip_x', 'hint_target': 5}, {'tooltip': 'ttip_y'}, {'target': 5}, {'target': 0, 'name': 'Hall'},
                         {'target': 9999, 'designId': 87}, {'target': 0, 'gate': 'voyagers'}, {'target': 0, 'gate': 'echoes'},
                         {'target': 0, 'gate': 'plutus', 'exit': True}, {'hint_target': 999, 'target': 5}, {}, {'target': 666},
                         {'gateId': 7, 'target': 13, 'designId': 100, 'gate': 'astral'}]
            qs += [{'op': 'tip', 'gate': dict(g, gateId=g.get('gateId', 100 + i))} for i, g in enumerate(synthetic)]
            qs += [{'op': 'asset', 'id': 151000123, 'tip': 'ttip_hangar', 'wild': []},
                   {'op': 'asset', 'id': 9, 'tip': 'ttip_instant_repair', 'wild': [['%COUNT%', 3], ['%X%', 'a']]}]
            for m in [1, 5, 92, 91, 93, 306, 307, 308, 200, 476, 29, 16]:
                ws, hs = (int(v) for v in json.loads(json.dumps(plain(run_size(tables, m)))))
                pts = [[rnd.uniform(-500, ws + 500), rnd.uniform(-500, hs + 500), rnd.choice([0, 50, 300]), rnd.choice([0, 60, 150])]
                       for _ in range(300)]
                qs.append({'op': 'points', 'map': m, 'points': pts})
                segs = [[rnd.randint(0, ws), rnd.randint(0, hs), rnd.randint(0, ws), rnd.randint(0, hs), rnd.choice([0, 60, 150])]
                        for _ in range(120)]
                qs.append({'op': 'segments', 'map': m, 'segments': segs})
                for seed in range(8):
                    qs.append({'op': 'random', 'map': m, 'seed': seed * 7919 + m, 'rect': [1000, 1000, 9000, 6000],
                               'pad': rnd.choice([0, 100, 150]), 'tries': rnd.choice([1, 5, 40]),
                               'probe': [[rnd.uniform(-1000, ws + 1000), rnd.uniform(-1000, hs + 1000)] for _ in range(20)]})
            json.dump(qs, open(qf, 'w'))
            py = run([sys.executable, '-I', os.path.join(HERE, 'oracle.py'), oo, scf, qf], env, os.path.join(tmp, 'python.json'))
            rs = run([exe, os.path.join(ROOT, 'data'), scf, qf], env, os.path.join(tmp, 'rust.json'))
            out = []
            if len(py) != len(rs):
                out.append(('answers', len(py), len(rs)))
            for i, (x, y) in enumerate(zip(py, rs)):
                q = qs[i]
                diff(x, y, '%s%s' % (q['op'], '@%s' % q['map'] if 'map' in q else ''), out)
            n = leaves(py)
            total += n
            bad += len(out)
            print('%-16s %6d queries %8d values  %s' % (name, len(qs), n, 'OK' if not out else '%d DIFFERENCES' % len(out)))
            for o in out[:15]:
                print('    ', str(o)[:400])
    finally:
        if not a.keep:
            shutil.rmtree(tmp, ignore_errors=True)
    print('%d values compared, %d differences' % (total, bad))
    sys.exit(1 if bad else 0)


def run_size(tables, m):
    s = tables['scale'].get(str(m), 1.0)
    return [int(21000 * s), int(13100 * s)]


if __name__ == '__main__':
    main()
