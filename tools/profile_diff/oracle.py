"""Answers profile cases with OpenOrbit's Profile.patch_template and the systems it reads (extras, booster, technology,
ship_ability, combat_items) — the reference for crates/lo-game/examples/profile_query.rs.

Usage: python3 -I oracle.py <OpenOrbit checkout> <cases.json> <out.json> [<prepared.json>]
Each case: a profile document (loaded with Profile(d=...), so OpenOrbit's migrations run; the document after loading is written
to <prepared.json> for the Rust side), a moment, panel settings, session hooks and daily fields. Answers: every message of the
login template patched in order, then the shortcut menu's cached states, the reported states, the jump panel, the booster
update, the extras' uses and charges, the timers of the combat items, technology and abilities, and the profile document after. The daily rank is computed
first, as at login.
"""
import copy, json, os, sys, time, types

oo, cases_path, out_path = (os.path.abspath(a) for a in sys.argv[1:4])
prepared_path = os.path.abspath(sys.argv[4]) if len(sys.argv) > 4 else None
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
NOW = [1763900000.0]
_real_time = time.time
time.time = lambda: NOW[0]
_real_localtime = time.localtime
time.localtime = lambda t=None: _real_localtime(NOW[0] if t is None else t)
import settings as SET  # noqa: E402
import player_profile as PP  # noqa: E402
import booster as BOOSTER  # noqa: E402
import extras as EX  # noqa: E402
import technology as TECH  # noqa: E402
import ship_ability as SA  # noqa: E402
import combat_items as CIT  # noqa: E402
import ranks as RT  # noqa: E402

tpl = json.load(open(os.path.join(oo, 'server', 'data', 'defaults', 'login.json'), encoding='utf8'))
seed = tpl.get('seed', {})
cases = json.load(open(cases_path, encoding='utf8'))
out, prepared = [], []
for c in cases:
    NOW[0] = c['now']
    sp = c.get('settings_path')
    SET.configure(path=sp or False)
    PP._EVENT_OPEN.clear()
    p = PP.Profile(seed=seed, path='/nonexistent/x.json', d=copy.deepcopy(c['doc']))
    prepared.append(copy.deepcopy(p.d))
    p.window_hooks = {k: (lambda v=v: v) for k, v in (c.get('hooks') or {}).items()}
    if c.get('daily'):
        p.daily_fields = (lambda d=c['daily']: dict(d))
    rank = [RT.daily_rank(p, force=bool(c.get('force_rank'))), RT.breakdown(p)]
    msgs = []
    for kind in ('entry', 'ready'):
        for m in tpl[kind]:
            if m['_m'].startswith('@'):
                continue
            msgs.append(p.patch_template(m['_m'], m))
    a = {'rank': rank, 'msgs': msgs,
         'cache': {k: p.menu_status(k) for k in sorted(p.menu_status_cache)},
         'sent': p.menu_sent, 'cpu': p.cpu_init_sent,
         'booster': BOOSTER.update_cmd(p, removed=c.get('removed', []))[1],
         'uses': [[EX.uses(p, l), EX.charges(p, l), EX.owned(p, l), EX.unlimited(p, l)] for l in c['loots']],
         'cit': [CIT.timer_fields(p, l) for l in c['timer_loots']],
         'tech': [TECH.timer_fields(p, t) for t in c['techs']],
         'sa': [[SA.timer_fields(p, s), SA.duration(p, s) if s in SA.ship_items(p) else None] for s in c['abilities']],
         'doc': p.d, 'dirty': p.dirty}
    out.append(a)
json.dump(out, open(out_path, 'w'))
if prepared_path:
    json.dump(prepared, open(prepared_path, 'w'))
