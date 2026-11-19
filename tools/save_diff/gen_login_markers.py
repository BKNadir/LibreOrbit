"""Writes what OpenOrbit's Profile.generate gives for the login markers that need the game's systems (@gemi, @arayuz,
@isaret), for crates/lo-game/tests/login_markers.rs.

The reference profiles (crates/lo-save/tests/fixtures/*.json) are varied (gates done, skills, P.E.T., dragged bars, a repair
robot on a drone, a portal, float positions) and asked under several panel settings (GM console, admin mode and ship, guest),
stations, session hooks and moments. Each case keeps the profile as OpenOrbit holds it after loading (its migrations done),
so the Rust side starts from the same document. Moments are kept only if every time zone in TZS gives the same @arayuz:
the Rust test then does not depend on the machine's time zone.

Usage: python3 -I gen_login_markers.py <OpenOrbit checkout> <profiles folder> <output.json>
"""
import calendar, copy, json, os, sys, tempfile, time, types

oo, fixtures, out_path = (os.path.abspath(a) for a in sys.argv[1:4])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
import settings as SET  # noqa: E402
import event_calendar as CAL  # noqa: E402
import player_profile as PP  # noqa: E402

TZS = ['Europe/Brussels', 'UTC', 'America/New_York', 'Asia/Tokyo', 'Pacific/Kiritimati', 'Pacific/Pago_Pago']
NOW = [0.0]
CAL.time = types.SimpleNamespace(time=lambda: NOW[0], timezone=time.timezone, altzone=time.altzone, localtime=time.localtime)
seed = json.load(open(os.path.join(oo, 'server', 'data', 'defaults', 'login.json'), encoding='utf8')).get('seed', {})
skills = json.load(open(os.path.join(oo, 'server', 'data', 'skills.json'), encoding='utf8'))['skills']
eng = len(skills['13']['values'])


def variants(d):
    yield 'base', d
    v = copy.deepcopy(d)
    v['gates'] = {'gates': {'alpha': {'done': 1}, 'gamma': {'done': True}, 'kronos': {'done': 3}, 'hades': {'done': True},
                            'beta': {'done': 0}, 'delta': {}}}
    v['skills'] = {'level': {'13': eng}, 'points': 0, 'converted': 0, 'resets': 0}
    v['pet'] = {'name': 'P.E.T.'}
    v['position'] = dict(v['position'], x=1234.9, y=-0.5, at_portal=True)
    v['equipment'] = dict(v['equipment'], extra=['equipment_extra_cpu_jp-01'])
    v['drone_equipment'] = [[1, ['equipment_weapon_laser_lf-3']], [2, ['equipment_extra_repbot_rep-4', 'equipment_extra_cpu_x']]]
    v.setdefault('settings', {})['windowSettingsModule'] = {
        'gameFeatureMenuPosition': '10,20', 'gameFeatureMenuOrder': 3, 'genericFeatureMenuPosition': '',
        'genericFeatureMenuOrder': '5'}
    yield 'varied', v
    v = copy.deepcopy(v)
    v['skills']['level']['13'] = eng - 1
    v['pet'] = {}
    v['settings']['windowSettingsModule'] = {'gameFeatureMenuOrder': '6', 'genericFeatureMenuOrder': 2.0,
                                             'genericFeatureMenuPosition': '1,2'}
    v['position'].pop('at_portal')
    yield 'varied2', v


SETTINGS = [('none', {}), ('gm', {'gm_console': True}), ('admin', {'admin_mode': True, 'admin_ship': 2}),
            ('admin own ship', {'admin_mode': True}), ('both', {'gm_console': True, 'admin_mode': True, 'admin_ship': 4})]
STATIONS = [None, {'posX': 1000, 'posY': 0, 'peaceRadius': 600, 'pvpPeace': True},
            {'posX': 1234, 'posY': 0, 'peaceRadius': 10, 'pvpPeace': False}, {'posX': 99999, 'posY': 0, 'peaceRadius': 10}]
HOOKS = [{}, {'returnee_login': True, 'battle_pass': False, 'plutus': True}]


def arayuz(p):
    out = []
    for tz in TZS:
        os.environ['TZ'] = tz
        time.tzset()
        CAL._ACTIVE[:] = [None, None]
        out.append(p.generate('@arayuz'))
    os.environ['TZ'] = TZS[0]
    time.tzset()
    return out[0] if all(o == out[0] for o in out) else None


# moments: one in each month of 2026-2027 at noon UTC on the 15th, plus a few chosen ones
moments = [float(calendar.timegm((y, m, 15, 12, 0, 0))) for y in (2026, 2027) for m in range(1, 13, 2)] + [1763900000.0, 1798000000.0]
tmp = tempfile.mkdtemp()
cases, profiles, uis = [], {}, []


def summary(ui):
    """What @arayuz changes in the saved interface: each bar's type, position, order and items (id, visible, maximized)."""
    return [[b.get('featureBarType'), b.get('featureBarPosition'), b.get('featureBarOrder'),
             [[it.get('itemId'), it.get('visible'), it.get('maximized')] for it in b.get('featureBarItems', [])]]
            for b in ui.get('featureBars', [])]


def ui_index(ui, full):
    """The layout in full (the first moment of a case), otherwise its summary."""
    v = ui if full else {'summary': summary(ui)}
    if v not in uis:
        uis.append(v)
    return uis.index(v)


for name in sorted(os.listdir(fixtures)):
    if not name.endswith('.json'):
        continue
    d0 = json.load(open(os.path.join(fixtures, name), encoding='utf8'))
    for vname, d in variants(d0):
        for sname, sv in SETTINGS:
            sp = os.path.join(tmp, 'settings.json')
            json.dump(sv, open(sp, 'w'))
            SET.configure(path=sp)
            for guest in (False, True):
                if guest and sname == 'none':
                    continue
                p = PP.Profile(seed=seed, path=os.path.join(tmp, 'x.json'), d=copy.deepcopy(d))
                doc = copy.deepcopy(p.d)
                if guest:
                    p.guest = 'guest'
                pid = '%s/%s' % (name, vname)
                assert profiles.setdefault(pid, doc) == doc
                case = {'name': '%s/%s%s' % (pid, sname, '/guest' if guest else ''), 'profile': pid,
                        'settings': sv, 'guest': guest, 'gemi': p.generate('@gemi')[1], 'isaret': []}
                pos = doc['position']
                edge = {'posX': pos['x'] - 500, 'posY': pos['y'], 'peaceRadius': 500, 'pvpPeace': 1}
                for st in STATIONS + [edge]:
                    q = PP.Profile(seed=seed, path=os.path.join(tmp, 'x.json'), d=copy.deepcopy(doc))
                    case['isaret'].append({'station': st, 'beacon': q.generate('@isaret', st)[1],
                                           'skills_after': q.d.get('skills')})
                case['beacon_moves'] = [
                    {'args': a, 'beacon': p.beacon(**dict(a, station=a.get('station'), pos=tuple(a['pos']) if a.get('pos') else None))}
                    for a in ({'pos': [10.7, 20.2], 'repairing': True, 'radiation': True},
                              {'station': STATIONS[1], 'pos': [1000, 100], 'void': True},
                              {'station': STATIONS[1], 'pos': [1000, 100], 'gate_peace': True, 'near_gate': True})]
                if sname == 'none' and (name == 'after_login.json' or vname == 'base'):
                    case['arayuz'] = []
                    for hooks in HOOKS:
                        p.window_hooks = {k: (lambda v=v: v) for k, v in hooks.items()}
                        for m in moments:
                            NOW[0] = m
                            r = arayuz(p)
                            if r is not None:
                                case['arayuz'].append({'now': m, 'hooks': hooks, 'ui': ui_index(r[1], not case['arayuz'])})
                cases.append(case)
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, 'w', encoding='utf8') as fh:
    json.dump({'tzs': TZS, 'profiles': profiles, 'uis': uis, 'cases': cases}, fh, ensure_ascii=False, separators=(',', ':'))
print('%d cases, %d @arayuz moments, %d distinct layouts' % (len(cases), sum(len(c.get('arayuz', [])) for c in cases), len(uis)))
