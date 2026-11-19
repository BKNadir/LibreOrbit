"""Compares LibreOrbit's settings (crates/lo-core/src/settings.rs) with OpenOrbit's settings.py: random settings documents
(valid, out-of-range, wrong-type and text values, sub-settings, choices, numbers, event choices) are cleaned and written by
both (file text byte for byte), then every multiplier with every source, the scaling helpers (price, count, duration,
amount with the same random seed, chance) and the panel's summary are compared.

Usage: python3 -I compare.py <OpenOrbit checkout> [--cases N]     Exit status 1 if anything differs.
"""
import argparse, json, os, random, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('openorbit')
    ap.add_argument('--cases', type=int, default=400)
    a = ap.parse_args()
    oo = os.path.abspath(a.openorbit)
    sys.path.insert(0, os.path.join(oo, 'server'))
    os.chdir(os.path.join(oo, 'server'))
    import settings as S
    rnd = random.Random(1)
    keys = [s['key'] for s in S.SCHEMA]
    junk = [None, True, False, 0, 1, -1, 2.5, -3.75, 1e9, 'x', '', '2', ' 3.5 ', '1_0', 'inf', '-inf', 'nan', [], {}, [1], 7.0, 0.0, 99999]
    qs = [{'doc': {}, 'seed': 0}, {'doc': {'events': {}}, 'seed': 1}, {'doc': [], 'seed': 2}]
    for i in range(a.cases):
        doc = {}
        for k in rnd.sample(keys + ['unknown', 'events'], rnd.randint(0, 25)):
            s = S.BY_KEY.get(k)
            if k == 'events':
                doc[k] = {e: rnd.choice(['on', 'off', 'maybe', None, True]) for e in rnd.sample(['halloween', 'winter', 'agatus', 'x'], rnd.randint(0, 4))}
            elif s and rnd.random() < 0.6:
                if s['type'] == 'multiplier':
                    doc[k] = rnd.choice([round(rnd.uniform(-1, s['cap'] * 1.2), 3), rnd.choice([0.5, 1.0, 2, 3.0])])
                elif s['type'] == 'bool':
                    doc[k] = rnd.choice([True, False, 1, 0])
                else:
                    doc[k] = rnd.randint(-2, 12)
            else:
                doc[k] = rnd.choice(junk)
        qs.append({'doc': doc, 'seed': i})
    tmp = tempfile.mkdtemp()
    qf = os.path.join(tmp, 'q.json')
    json.dump(qs, open(qf, 'w'))
    sources = [None] + [s['source'] for s in S.SCHEMA if s.get('source')] + ['nope']
    py = []
    for q in qs:
        S.configure(path=False)
        S._state['d'] = S.clean(q['doc'])
        p = os.path.join(tmp, 'w.json')
        S.write(p, q['doc'])
        text = open(p, encoding='utf8').read()
        mult = [S.multiplier(s['key'], src) for s in S.SCHEMA if not s.get('parent') and s['type'] == 'multiplier' for src in sources]
        random.seed(q['seed'])
        helpers = [[S.price('shop_price', n, 'uridium'), S.count('npc_count', n, 'map'), S.duration(n, 'skylab'),
                    S.amount('uridium', n, 'npc'), S.chance('drop', n / 50.0, 'booster')] for n in (0, 1, 3, 7, 100, 12345, -5)]
        py.append({'text': text, 'mult': mult, 'helpers': helpers, 'rng': random.random(), 'summary': [list(x) for x in S.summary()]})
    subprocess.run(['cargo', 'build', '--quiet', '--release', '-p', 'lo-core', '--example', 'settings_query'], cwd=ROOT, check=True)
    rs = json.loads(subprocess.run([os.path.join(ROOT, 'target', 'release', 'examples', 'settings_query'), qf], capture_output=True,
                                   text=True, check=True).stdout)
    bad = 0
    for q, x, y in zip(qs, py, rs):
        for k in x:
            same = x[k] == y[k] and json.dumps(x[k]) == json.dumps(y[k])
            if not same:
                bad += 1
                if bad <= 10:
                    print('DIFF', k, json.dumps(q['doc'])[:300], '\n  python:', json.dumps(x[k])[:300], '\n  rust:  ', json.dumps(y[k])[:300])
    n = sum(len(x['mult']) + len(x['helpers']) * 5 + len(x['summary']) + 2 for x in py)
    print('%d documents, %d values compared, %d differences' % (len(qs), n, bad))
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
