"""Compares LibreOrbit's patch_template and the systems it reads (crates/lo-game/src/templates.rs, extras.rs, booster.rs,
technology.rs, ship_ability.rs, combat_items.rs) with OpenOrbit's Profile.patch_template, extras.py, booster.py,
technology.py, ship_ability.py and combat_items.py.

Random profiles are built from a reference profile: extras fitted, in the inventory or on drones (jump, advanced jump, cloak,
aiming and other on/off CPUs, trade drone, repair robots, slot CPUs), rocket launchers and their load, drones and formations,
ammo, mines, Xenomit, coupons, boosters (old and new form), technology counts and timers, ship abilities (ships with toggles,
Last Stand, designs), cooldown groups, maps (start maps, Galaxy Gates, Echoes, special maps), shortcuts and dragged bars,
cloaking, invite block, P.E.T., laboratory, unlimited trade drone designs; at random moments, with panel settings, session
hooks and daily fields. Every message of the login template is patched in order on both sides (the shortcut menu included),
then the cached and reported menu states, the jump panel, the booster update, the extras' uses, the timers and the profile
after are compared.

Usage: python3 -I compare.py <OpenOrbit checkout> [--cases N] [--seed S] [--keep <dir>]     Exit status 1 if anything differs.
"""
import argparse, copy, json, os, random, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BASE = os.path.join(ROOT, 'crates', 'lo-save', 'tests', 'fixtures', 'after_login.json')


def tagged(path):
    return json.load(open(path), parse_float=lambda s: ('float', float(s)), parse_int=lambda s: ('int', int(s)))


def diff(x, y, path, out, ordered=True):
    if type(x) is not type(y):
        out.append((path, x, y))
    elif isinstance(x, dict):
        for k in list(x) + [k for k in y if k not in x]:
            if k not in x or k not in y:
                out.append((path + '/' + k, str(x.get(k, '<missing>'))[:200], str(y.get(k, '<missing>'))[:200]))
            else:
                diff(x[k], y[k], path + '/' + k, out)
        if ordered and set(x) == set(y) and list(x) != list(y):
            out.append((path, 'key order', list(x)[:10], list(y)[:10]))
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


def make_cases(oo, n, seed, tmp):
    E = json.load(open(os.path.join(oo, 'server', 'data', 'items.json'), encoding='utf8'))
    SA = json.load(open(os.path.join(oo, 'server', 'data', 'ship_abilities.json'), encoding='utf8'))
    base = json.load(open(BASE, encoding='utf8'))
    rnd = random.Random(seed)
    extras = sorted(E['extras']) + list(E['repair_robots'])
    launchers = sorted(E['combat_items']['launcher']['kinds'])
    mines = sorted(E['combat_items']['mines'])
    rockets = sorted(E['rockets'])
    lammo = sorted(E['combat_items']['launcher_ammo'])
    techs = sorted(E['technology']['items'])
    boosters = sorted(E['boosters']['kinds'])
    formations = sorted(E['formations'])
    ships = sorted(SA['ships'])
    abilities = sorted(SA['skills'])
    groups = [1, 2, 3, 9, 47, 53, 56, 57, 58]
    timer_loots = mines + ['ammunition_specialammo_emote_hi', 'ammunition_specialammo_spray_cross', 'ammunition_mine_smb-01',
                           'equipment_extra_cpu_ish-01', 'ammunition_specialammo_emp-01', 'ammunition_specialammo_ghostifier',
                           'drone_formation_f-02-ar', 'ammunition_specialammo_alien-cpu-common',
                           'ammunition_specialammo_bountiful-alien-cpu-normal'] + [r for r in rockets if E['rockets'][r].get('special')] + \
                  ['ammunition_laser_lcb-10', 'nope']
    settings_files = []
    for i, s in enumerate([{'booster_power': 2.0}, {'booster_power': 0.5, 'no_level_limit': True}, {'events': {'agatus': 'on', 'chromin': 'on'}}]):
        p = os.path.join(tmp, 'settings%d.json' % i)
        json.dump(s, open(p, 'w'))
        settings_files.append(p)
    cases = []
    for i in range(n):
        d = copy.deepcopy(base)
        now = rnd.uniform(1767225600, 1798761600)            # 2026-2027
        sid = d['active_ship']
        ship = d['ships'][sid]
        cid = str(ship['active_config'])
        nid = [d['next_id']]

        def new(loot):
            k = str(nid[0])
            nid[0] += 1
            d['items'][k] = loot
            return int(k)

        cfg = ship['configs'].setdefault(cid, {})
        for loot in rnd.sample(extras, rnd.randint(0, 9)):
            iid = new(loot)
            if rnd.random() < 0.7:
                cfg.setdefault('extra', []).append(iid)
            if rnd.random() < 0.2:
                new(loot)                                 # a second copy in the inventory
        if rnd.random() < 0.6:
            for loot in rnd.sample(launchers, rnd.randint(1, 2)):
                cfg.setdefault('rocket', []).append(new(loot))
            if rnd.random() < 0.7:
                d['launcher_loaded'] = [rnd.choice(lammo), rnd.randint(0, 6)]
            if rnd.random() < 0.7:
                d['selected']['launcher'] = rnd.choice(lammo + [None])
        nd = rnd.choice([0, 0, 3, 4, 6, 10])
        d['drones'] = [{'uid': j + 1, 'lootId': rnd.choice(['drone_iris', 'drone_flax']), 'level': rnd.randint(1, 6), 'points': 0}
                       for j in range(nd)]
        d['next_drone'] = nd + 1
        dc = ship.setdefault('drone_config', {}).setdefault(cid, {})
        for x in d['drones']:
            if rnd.random() < 0.3:
                dc[str(x['uid'])] = [new(rnd.choice(extras + ['equipment_weapon_laser_lf-3']))]
        d['formations'] = rnd.sample(formations, rnd.randint(0, 5))
        d['formation'] = rnd.choice([0, 1, 2, 3, 4, 5, 7])
        for loot in rnd.sample(mines + rockets + lammo + ['ammunition_laser_lcb-10', 'ammunition_laser_ucb-100'], 10):
            d['ammunition'][loot] = rnd.choice([0, 0, 5, 37, 1000])
        d['ores'] = [{'_': 'OreCountModule', 'oreType': {'_': 'OreTypeModule', 'typeValue': t}, 'count': rnd.randint(0, 900)}
                     for t in sorted(rnd.sample([0, 1, 2, 3, 4, 5, 6, 7, 8], 4))]
        if rnd.random() < 0.5:
            d.setdefault('other_items', {})['resource_jumpvoucher'] = rnd.randint(0, 9)
        d['boosters'] = {b: ({'end_time': now + rnd.uniform(-500, 5000), 'remaining': rnd.choice([0, 300.5])} if rnd.random() < 0.3
                             else {'end_time': 0, 'remaining': rnd.choice([0, 12.5, 3600, 86400.0])})
                         for b in rnd.sample(boosters, rnd.randint(0, 25))}
        if i % 5 == 0:                                    # every booster at once: the caps
            d['boosters'] = {b: {'end_time': 0, 'remaining': 3600} for b in boosters}
        d['technology'] = {t: rnd.choice([0, 1, 4]) for t in rnd.sample(techs, rnd.randint(0, 6))}
        d['technology_state'] = {t: {'end_time': now + rnd.uniform(-100, 100), 'duration_ms': 9000, 'ready': now + rnd.uniform(-100, 400),
                                     'cooldown_ms': 60000} for t in rnd.sample(techs, rnd.randint(0, 4))}
        if rnd.random() < 0.8:
            ship['lootId'] = rnd.choice(ships)
        if rnd.random() < 0.3:
            ship['design'] = rnd.choice(ships + ['ship_goliath_design_amber'])
        if rnd.random() < 0.3:
            ship['designs'] = ['ship_goliath_design_amber']
        d['ship_abilities'] = {a: {'end_time': now + rnd.uniform(-60, 60), 'duration_ms': 5000, 'ready': now + rnd.uniform(-60, 200),
                                   'cooldown_ms': 120000, 'on': rnd.random() < 0.5, 'built': rnd.random() < 0.5}
                               for a in rnd.sample(abilities, 12)}
        d['cooldown_groups'] = {str(g): [now + rnd.uniform(-30, 90), rnd.choice([30000, 60000])] for g in rnd.sample(groups, 5)}
        d['position']['mapId'] = rnd.choice([1, 5, 9, 2, 13, 16, 51, 71, 300, 42, 200, 91, 306, 445, 0])
        d['shortcuts'] = {'standardSlotBar': {str(k): rnd.choice(mines + rockets) for k in rnd.sample(range(1, 11), 4)}}
        if rnd.random() < 0.5:
            d['shortcuts']['premiumSlotBar'] = {'3': 'ammunition_laser_lcb-10', '1': 'ability_x'}
        d['settings']['windowSettingsModule'] = rnd.choice([{}, {'slotmenuPosition': '10,20|5,5', 'slotMenuOrder': 3, 'mainmenuPosition': '1,2',
                                                                  'slotmenuPremiumPosition': '', 'slotMenuProActionOrder': '7',
                                                                  'gameFeatureMenuPosition': '3,4'}])
        d['cpu_hidden'] = rnd.random() < 0.3
        d['invite_block'] = rnd.choice([True, False, None])
        d['pet'] = rnd.choice([None, {}, {'fuel': 0, 'destroyed': False}, {'fuel': 50, 'destroyed': rnd.random() < 0.5}])
        d['upgrades'] = {str(k): [rnd.randint(1, 3), rnd.randint(1, 900)] for k in rnd.sample(range(4), rnd.randint(0, 3))}
        if rnd.random() < 0.5:
            d['extra_uses'] = {l: rnd.randint(0, 120) for l in rnd.sample(extras, 4)}
            d['extra_copies'] = {l: rnd.randint(1, 3) for l in rnd.sample(extras, 3)}
        d['next_id'] = nid[0]
        cases.append({'doc': d, 'now': now, 'settings_path': rnd.choice([None, None] + settings_files),
                      'hooks': rnd.choice([{}, {'plutus': True, 'tdm': False, 'returnee_login': True}]),
                      'daily': rnd.choice([None, {'day': 3, 'claimed': True}]),
                      'removed': rnd.sample(boosters, rnd.randint(0, 2)),
                      'loots': rnd.sample(extras, 8), 'timer_loots': timer_loots, 'techs': techs,
                      'abilities': rnd.sample(abilities, 15)})
    return cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('openorbit')
    ap.add_argument('--cases', type=int, default=60)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--keep')
    a = ap.parse_args()
    oo = os.path.abspath(a.openorbit)
    subprocess.run(['cargo', 'build', '--quiet', '--release', '-p', 'lo-game', '--example', 'profile_query'], cwd=ROOT, check=True)
    tmp = a.keep or tempfile.mkdtemp(prefix='profile_diff_')
    os.makedirs(tmp, exist_ok=True)
    try:
        cases = make_cases(oo, a.cases, a.seed, tmp)
        cf, pyf, prep, rsf = (os.path.join(tmp, f) for f in ('cases.json', 'python.json', 'prepared.json', 'rust.json'))
        json.dump(cases, open(cf, 'w'))
        env = {k: v for k, v in os.environ.items() if k != 'OO_WORLD'}
        r = subprocess.run([sys.executable, '-I', os.path.join(HERE, 'oracle.py'), oo, cf, pyf, prep], env=env, capture_output=True, text=True)
        if r.returncode:
            raise SystemExit('oracle failed:\n' + r.stderr)
        with open(rsf, 'w') as fh:
            r = subprocess.run([os.path.join(ROOT, 'target', 'release', 'examples', 'profile_query'), os.path.join(ROOT, 'data'), cf, prep],
                               env=env, stdout=fh, stderr=subprocess.PIPE, text=True)
        if r.returncode:
            raise SystemExit('profile_query failed:\n' + r.stderr)
        py, rs = tagged(pyf), tagged(rsf)
        out = []
        for i, (x, y) in enumerate(zip(py, rs)):
            for k in x:
                ordered = k not in ('sent',)
                if k == 'sent':
                    x[k], y[k] = dict(sorted(x[k].items())), dict(sorted(y[k].items()))
                diff(x[k], y[k], 'case%d/%s' % (i, k), out, ordered)
        n = leaves(py)
        print('%d cases, %d values compared, %d differences' % (len(py), n, len(out)))
        for o in out[:25]:
            print('    ', str(o)[:500])
        sys.exit(1 if out else 0)
    finally:
        if not a.keep:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == '__main__':
    main()
