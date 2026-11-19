"""Answers event calendar queries with OpenOrbit's event_calendar.py (the reference for crates/lo-game/examples/calendar_query.rs).

Usage: python3 -I oracle.py <OpenOrbit checkout> <scenario.json> <queries.json>
The World Editor world comes from OO_WORLD, the time zone from TZ (both read by the caller's environment).
"""
import datetime, json, os, sys

oo, scenario, queries = (os.path.abspath(a) for a in sys.argv[1:4])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
sc = json.load(open(scenario, encoding='utf8'))
qs = json.load(open(queries, encoding='utf8'))

import settings as SET  # noqa: E402
SET.configure(path=sc.get('settings') or False)
import event_calendar as CAL  # noqa: E402
CAL.configure(['game.py'] + sc.get('argv', []))
for k, r in (sc.get('pass_runs') or {}).items():
    CAL.PASS_RUNS[k] = tuple(r)


def sections(k):
    e = CAL.EVENTS[k]
    return ([e['map']] if isinstance(e.get('map'), dict) else []) + e.get('map_extra', [])


def ivs(v):
    return [[a.isoformat(), b.isoformat()] for a, b in v]


def cmd(c):
    name, d = c
    return dict({'_': name}, **d)


dt = datetime.datetime.fromisoformat
out = []
for q in qs:
    op, key, now = q['op'], q.get('key'), q.get('now')
    if op == 'events':
        a = CAL.EVENTS
    elif op == 'conflicts':
        a = {k: sorted(CAL.CONFLICTS.get(k, ())) for k in CAL.EVENTS}
    elif op == 'known':
        a = [CAL.KNOWN.isoformat(), CAL.ALWAYS]
    elif op == 'holidays':
        y = q['year']
        a = [str(CAL._easter(y)), str(CAL._thanksgiving(y)), str(CAL._cny(y))]
    elif op == 'season':
        a = CAL.season(dt(q['t']))
    elif op == 'instant':
        a = {}
        active = CAL.active(now)
        for k in CAL.EVENTS:
            p = CAL.period(k, now)
            a[k] = [list(p) if p else None, CAL._on(k, now), CAL.is_active(k, now), CAL.forced(k), CAL.time_left(k, now),
                    CAL.news(k, now)]
            a[k + '/window'] = [CAL.window(s, now) for s in sections(k)]
        a['_active'] = sorted(active)
        a['_cmd'] = cmd(CAL.event_cmd(now))
        a['_moods'] = [cmd(c) for c in CAL.mood_cmds(active)]
        a['_achievements'] = sorted(CAL.achievement_groups(now))
        a['_types'] = [CAL.has_type(t, now) for t in q['types']]
        a['_gates'] = {g: [[list(r) for r in CAL.gate_rewards(g, now)], CAL.gate_choices(g, now), CAL.gate_bundles(g, now)]
                       for g in q['gates']}
    elif op == 'mood_change':
        a = [cmd(c) for c in CAL.mood_cmds(set(q['keys']), set(q['before']))]
    elif op == 'upcoming':
        a = {k: ivs(CAL.upcoming(k, now, q['days'])) for k in CAL.EVENTS}
    elif op == 'intervals':
        x, y = dt(q['a']), dt(q['b'])
        a = {}
        for k in CAL.EVENTS:
            a[k] = ivs(CAL.intervals(k, x, y))
            a[k + '/sections'] = [[ivs(CAL.section_intervals(s, x, y)), ivs(CAL.section_intervals(s, x, y, True))]
                                  for s in sections(k)]
    elif op == 'section_window':
        a = CAL.window(sections(key)[q['idx']], now)
    else:
        raise SystemExit('unknown op %s' % op)
    out.append(a)
print(json.dumps(out))
