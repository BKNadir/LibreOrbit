"""Compares LibreOrbit's event calendar (crates/lo-game/src/event_calendar.rs) with OpenOrbit's event_calendar.py.

Both answer the same queries (tools/calendar_diff/oracle.py for Python, the calendar_query example for Rust) in the same
time zone, for several scenarios (panel choices, --event, Battle Pass runs, a World Editor world) and several time zones:
for every event at each moment period, _on, is_active, forced, time_left, news, the map sections' windows; and active,
event_cmd, mood_cmds, has_type, achievement groups, gate rewards; plus intervals, section_intervals and upcoming over
ranges, the holidays, the seasons and the loaded event data. Moments: random ones, a grid, the edges of every period
(found by OpenOrbit's own intervals, probed one second before, at and after) and the daylight saving changes.

Usage: python3 compare.py <OpenOrbit checkout> [--tz Europe/Brussels,...] [--quick] [--only <scenario>] [--keep <dir>]
Exit status 1 if anything differs.
"""
import argparse, datetime, json, os, random, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
TZS = ['Europe/Brussels', 'America/New_York', 'Australia/Lord_Howe', 'UTC']
TYPES = ['CandyCargo', 'UridiumBank', 'NpcEvent', 'BattlePass', 'MiniClientReward', 'PirateBootyRedAndBlue', 'StrikeAndExit',
         'GhostBosses', 'Nope']
GATES = ['alpha', 'beta', 'gamma', 'delta', 'epsilon', 'zeta', 'kappa', 'lambda', 'streuner']
WORLD = {
    'name': 'calendar diff',
    'patches': {'events.json': {
        '/known_calendar': '2026-06-30',
        '/history_start': '2024-01-01',
        '/events/halloween/world_status': 'on',
        '/events/winter/world_status': 'off',
        '/events/agatus/periods/0': ['2023-01-18', '2023-01-30'],
        '/events/sunburst/periods': [['06-21', '06-23', [[10, 14]]], ['easter-1', 'easter+1', [[20, 27.5]]]],
        '/events/vip_merchant': {'$delete': True},
        '/events/dispatch_day/counts_as': 'kronos_day',
        '/events/kronos_day/nth_of_month': [1, 3],
        '/events/levelup_sale/week_pattern/span': 3,
        '/events/custom_test': {'name': 'Custom', 'kinds': ['CandyCargo', 'NpcEvent'], 'days': [2, 6], 'hours': [[22, 26]],
                                'block_cycle': [2, 1], 'news': ['custom{Month}{Y}', 'c{{x}}{mon}'],
                                'clan_news': ['customMarch2027', 'c{x}apr'], 'news_default': 'customDefault',
                                'news_extra': ['e1'], 'decor': {'type': 9, 'value': 5}, 'achievement': ['zz'],
                                'gate_reward': [{'gates': ['alpha'], 'items': [['x', 1, 0.5]]}]},
        '/events/custom_single': {'name': 'Single', 'kinds': ['BattlePass'], 'periods': [
            ['2026-12-24 18:30', '2027-01-03', 'single', [[0, 2], [18, 30]]], ['02-28', '03-01'], ['12-30', '01-02']],
            'together': ['winter', 'custom_test']},
    }},
}


def ts(y, m, d, h=0, mi=0):
    return datetime.datetime(y, m, d, h, mi).timestamp()


def scenarios(tmp):
    settings = os.path.join(tmp, 'settings.json')
    json.dump({'session_always_open': True, 'events': {
        'halloween': 'on', 'agatus': 'on', 'winter': 'off', 'gg_double_reward': 'off', 'prosperous': 'on', 'fever': 'on',
        'levelup_sale': 'off', 'world_boss': 'on', 'bogus': 'on', 'spring': 'maybe'}}, open(settings, 'w'))
    world = os.path.join(tmp, 'world.json')
    json.dump(WORLD, open(world, 'w'))
    passes = {'halloween': [ts(2026, 3, 1), ts(2026, 4, 15, 12)], 'world_boss': [ts(2026, 11, 10), ts(2027, 1, 10)],
              'galactic_strife': [ts(2025, 12, 1, 6), ts(2026, 2, 1)], 'custom_single': [ts(2026, 12, 1), ts(2026, 12, 20)]}
    return [
        ('standard', {}, None),
        ('panel choices', {'settings': settings}, None),
        ('--event all', {'argv': ['--event', 'all']}, None),
        ('--event none + names', {'argv': ['--event=none', '--event', 'winter,Spring, hitac,nope']}, None),
        ('forced conflicts + passes', {'argv': ['--event=halloween,agatus'], 'pass_runs': passes}, None),
        ('passes', {'pass_runs': passes}, None),
        ('world + panel + passes', {'settings': settings, 'pass_runs': passes}, world),
    ]


def run(cmd, env, out):
    with open(out, 'w') as fh:
        r = subprocess.run(cmd, env=env, stdout=fh, stderr=subprocess.PIPE, text=True)
    if r.returncode:
        raise SystemExit('%s failed:\n%s' % (cmd[0], r.stderr))
    return json.load(open(out), parse_float=lambda s: ('float', float(s)), parse_int=lambda s: ('int', int(s)))


def diff(x, y, path, out):
    if type(x) is not type(y):
        out.append((path, x, y))
    elif isinstance(x, dict):
        for k in list(x) + [k for k in y if k not in x]:
            if k not in x or k not in y:
                out.append((path + '/' + k, x.get(k, '<missing>'), y.get(k, '<missing>')))
            else:
                diff(x[k], y[k], path + '/' + k, out)
        if set(x) == set(y) and list(x) != list(y):
            out.append((path, 'key order', list(x)[:8]))
    elif isinstance(x, list):
        if len(x) != len(y):
            out.append((path, 'length %d' % len(x), 'length %d' % len(y)))
        for i, (p, q) in enumerate(zip(x, y)):
            diff(p, q, '%s[%d]' % (path, i), out)
    elif x != y:
        out.append((path, x, y))


def leaves(x):
    if isinstance(x, dict):
        return sum(leaves(v) for v in x.values())
    if isinstance(x, list):
        return sum(leaves(v) for v in x) or 1
    return 1


def dst_changes(a, b):
    """Moments where the local UTC offset changes, in [a, b)."""
    out, t = [], a
    off = time.localtime(t).tm_gmtoff
    while t < b:
        n = time.localtime(t + 3600).tm_gmtoff
        if n != off:
            lo, hi = t, t + 3600
            while hi - lo > 1:
                mid = (lo + hi) // 2
                lo, hi = (mid, hi) if time.localtime(mid).tm_gmtoff == off else (lo, mid)
            out.append(hi)
            off = n
        t += 3600
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('openorbit')
    ap.add_argument('--tz', default=','.join(TZS))
    ap.add_argument('--quick', action='store_true', help='fewer moments')
    ap.add_argument('--only', help='only the scenarios whose name contains this text')
    ap.add_argument('--keep', help='keep the query and answer files in this folder')
    a = ap.parse_args()
    oo = os.path.abspath(a.openorbit)
    subprocess.run(['cargo', 'build', '--quiet', '--release', '-p', 'lo-game', '--example', 'calendar_query'], cwd=ROOT,
                   check=True)
    exe = os.path.join(ROOT, 'target', 'release', 'examples', 'calendar_query')
    tmp = a.keep or tempfile.mkdtemp(prefix='calendar_diff_')
    os.makedirs(tmp, exist_ok=True)
    total, bad = 0, 0
    try:
        for tz in a.tz.split(','):
            os.environ['TZ'] = tz
            time.tzset()
            env = dict(os.environ, TZ=tz)
            for name, sc, world in scenarios(tmp):
                if a.only and a.only not in name:
                    continue
                rnd = random.Random('%s|%s' % (tz, name))
                envw = dict(env, OO_WORLD=world) if world else {k: v for k, v in env.items() if k != 'OO_WORLD'}
                scf = os.path.join(tmp, 'scenario.json')
                json.dump(sc, open(scf, 'w'))
                # the edges of every period, from OpenOrbit itself
                probe = os.path.join(tmp, 'probe.json')
                json.dump([{'op': 'intervals', 'a': '2022-01-01T00:00:00', 'b': '2036-01-01T00:00:00'}], open(probe, 'w'))
                edges = set()
                stack = [run([sys.executable, '-I', os.path.join(HERE, 'oracle.py'), oo, scf, probe], envw,
                             os.path.join(tmp, 'probe_out.json'))]
                while stack:
                    v = stack.pop()
                    if isinstance(v, str):
                        edges.add(datetime.datetime.fromisoformat(v).timestamp())
                    elif isinstance(v, (list, dict)):
                        stack += v.values() if isinstance(v, dict) else v
                edges = sorted(edges)
                lo, hi = ts(2022, 6, 1), ts(2035, 12, 31)
                n_rand, step, n_edges = (150, 6 * 3600 + 1234, 250) if a.quick else (600, 3 * 3600 + 1234, 900)
                moments = [rnd.uniform(lo, hi) for _ in range(n_rand)]
                moments += [float(t) for t in range(int(ts(2025, 10, 1)), int(ts(2027, 2, 1)), step)]
                for e in rnd.sample(edges, min(n_edges, len(edges))):
                    moments += [e - 1, e, e + 0.5]
                for c in dst_changes(int(ts(2024, 1, 1)), int(ts(2031, 12, 31))):
                    moments += [c + d for d in (-3600, -1801, -1, 0, 1, 1799, 3600)]
                moments.sort()
                qs = [{'op': 'events'}, {'op': 'conflicts'}, {'op': 'known'}]
                qs += [{'op': 'holidays', 'year': y} for y in range(1990, 2101)]
                qs += [{'op': 'season', 't': datetime.datetime.fromtimestamp(rnd.uniform(lo, hi)).replace(microsecond=0)
                        .isoformat()} for _ in range(50)]
                qs += [{'op': 'instant', 'now': m, 'types': TYPES, 'gates': GATES} for m in moments]
                qs += [{'op': 'upcoming', 'now': rnd.choice(moments), 'days': rnd.choice([366, 30, 7, 1])} for _ in range(25)]
                windows = [('2022-01-01T00:00:00', '2036-01-01T00:00:00')]
                for _ in range(25):
                    s = datetime.datetime.fromtimestamp(rnd.uniform(lo, hi)).replace(microsecond=0)
                    windows.append((s.isoformat(), (s + datetime.timedelta(days=rnd.uniform(0.2, 120))).replace(
                        microsecond=0).isoformat()))
                qs += [{'op': 'intervals', 'a': x, 'b': y} for x, y in windows]
                decor = ['agatus', 'winter', 'spring', 'pre_prometheus', 'prometheus', 'fiesta'] + (['custom_test'] if world else [])
                qs += [{'op': 'mood_change', 'keys': rnd.sample(decor, rnd.randint(0, 4)),
                        'before': rnd.sample(decor, rnd.randint(0, 4))} for _ in range(30)]
                qf = os.path.join(tmp, 'queries.json')
                json.dump(qs, open(qf, 'w'))
                py = run([sys.executable, '-I', os.path.join(HERE, 'oracle.py'), oo, scf, qf], envw,
                         os.path.join(tmp, 'python.json'))
                rs = run([exe, os.path.join(ROOT, 'data'), scf, qf], envw, os.path.join(tmp, 'rust.json'))
                out = []
                if len(py) != len(rs):
                    out.append(('answers', len(py), len(rs)))
                for i, (x, y) in enumerate(zip(py, rs)):
                    q = qs[i]
                    diff(x, y, '%s%s' % (q['op'], '@%r' % q['now'] if 'now' in q else ''), out)
                n = leaves(py)
                total += n
                bad += len(out)
                print('%-20s %-28s %5d moments %8d values  %s' % (tz, name, len(moments), n,
                                                                    'OK' if not out else '%d DIFFERENCES' % len(out)))
                for o in out[:15]:
                    print('    ', str(o)[:400])
    finally:
        if not a.keep:
            shutil.rmtree(tmp, ignore_errors=True)
    print('%d values compared, %d differences' % (total, bad))
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
