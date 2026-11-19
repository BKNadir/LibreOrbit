"""Writes what OpenOrbit's Profile._migrate makes of older or incomplete profiles, for crates/lo-save/tests/migrate.rs.

Cases: new profiles from three seeds (the login template's, an empty one, one without a recorded hangar), and the reference
profiles with old sections put back (equipment and ammo before the hangar, resources in the ammo, configs before ships,
drone designs on the drones, the P.E.T.'s config, the old saved designs, an Astral run with the player's ship in backup,
Osmium in storage, the byte-swapped and default quality settings, the old gate counter, the interface before the new
windows, upgrade levels, float counts), each with and without the panel's infinite cargo. A case keeps the document before,
the document OpenOrbit holds after (as written by json.dumps, key order kept) and whether the profile is marked as changed. A last case
creates a new profile file, to compare what Profile(...) writes at once.

Usage: python3 -I gen_migrate.py <OpenOrbit checkout> <profiles folder> <output.json>
"""
import copy, json, os, sys, tempfile

oo, fixtures, out_path = (os.path.abspath(a) for a in sys.argv[1:4])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
import settings as SET  # noqa: E402
import player_profile as PP  # noqa: E402

seed = json.load(open(os.path.join(oo, 'server', 'data', 'defaults', 'login.json'), encoding='utf8')).get('seed', {})
SEEDS = {'template': seed, 'empty': {}, 'no hangar': {k: v for k, v in seed.items() if k not in ('configs', 'items', 'ship_ids')}}
CREATED = '2026-01-01 00:00:00'


def new_doc(s):
    d = PP.Profile._from_seed(s)
    d['created'] = CREATED
    return d


def olden(d):
    """Variants of a migrated profile with old sections put back."""
    yield 'as saved', d
    v = copy.deepcopy(d)
    v['ammo_seeded'] = False
    v['ammunition'] = {'ammunition_laser_ucb-100': 50, 'resource_booty-key': 3, 'resource_zero': 0, 'ammunition_laser_lcb-10': 2.5}
    v.pop('selected', None)
    v.setdefault('other_items', {})['resource_booty-key'] = 1
    v['other_items']['resource_ore_osmium'] = 40
    v['ores'] = v.get('ores', []) + [{'_': 'OreCountModule', 'oreType': {'_': 'OreTypeModule', 'typeValue': 28}, 'count': 5},
                                      {'_': 'OreCountModule', 'oreType': {'_': 'OreTypeModule', 'typeValue': 3}, 'count': 900}]
    v['ship']['galaxyGatesDone'] = 3
    q = v['settings'].setdefault('qualitySettingsModule', {})
    q.update(qualityExplosion=773, notSet=False)
    v['upgrade_level'] = {k: 2 for k in list(v.get('items', {}))[:3]}
    yield 'old ammo, ores, quality, gates', v
    v = copy.deepcopy(d)
    q = v['settings'].setdefault('qualitySettingsModule', {})
    q.update(notSet=False, qualityPresetting=3, qualityShip=0, qualityEngine=0, qualityExplosion=0, qualityCollectables=0)
    for bar in v.get('ui', {}).get('featureBars', []):
        bar['featureBarItems'] = [it for it in bar.get('featureBarItems', []) if it.get('itemId') not in ('apotheosis', 'recallBattlePass')]
    sid = v['active_ship']
    ship = v['ships'][sid]
    ship.pop('drone_config', None)
    ship.pop('drone_design', None)
    ship['saved_drone_design'] = {'1': {'design': {'1': 5, '2': 0}, 'appearance': None}, '2': {'design': {'2': 7}}}
    items = list(v.get('items', {}))
    v['drones'] = [{'uid': 1, 'lootId': 'drone_iris', 'level': 3, 'points': 0, 'design': items[0] if items else 9, 'upgrade': 2},
                   {'uid': 2, 'lootId': 'drone_flax', 'level': 1, 'points': 0, 'appearance': {'1': 11, '2': 0}},
                   {'uid': 3, 'lootId': 'drone_iris', 'level': 1, 'points': 0, 'design': None}]
    v['next_drone'] = 4
    v['pet'] = {'name': 'P', 'config': {'1': {'gear': [1]}, '2': {}}}
    yield 'old designs, quality default, windows', v
    v = copy.deepcopy(v)
    other = str(int(sid) + 1)
    v['ships'][other] = {'lootId': 'ship_leonov', 'hangarId': 9, 'configs': {'1': {}, '2': {}}, 'active_config': 2, 'favorite': False}
    v['pve'] = {'astral': {'run': {'inside': True, 'backup': {'active': sid, 'drones': [
        {'uid': 5, 'lootId': 'drone_iris', 'level': 2, 'points': 0, 'design': {'1': 21}}]}}}}
    v['active_ship'] = other
    yield 'astral run', v
    v = copy.deepcopy(d)
    ship = v['ships'].pop(v.pop('active_ship'))
    v['configs'] = ship['configs']
    v['active_config'] = ship['active_config']
    for k in ('next_id', 'drones', 'formations', 'next_drone', 'upgrades', 'quests', 'equipment', 'drone_equipment'):
        v.pop(k, None)
    v.pop('ships')
    yield 'configs before ships', v


cases = []
tmp = tempfile.mkdtemp()
docs = [('seed %s' % name, s, new_doc(s)) for name, s in SEEDS.items()]
for name in sorted(os.listdir(fixtures)):
    if name.endswith('.json') and name == 'after_login.json':
        d0 = json.load(open(os.path.join(fixtures, name), encoding='utf8'))
        docs += [('%s/%s' % (name, vname), seed, v) for vname, v in olden(d0)]
for cargo in (False, True):
    sp = os.path.join(tmp, 'settings.json')
    json.dump({'infinite_cargo': cargo}, open(sp, 'w'))
    SET.configure(path=sp)
    for name, s, doc in docs:
        p = PP.Profile(seed=s, path=os.path.join(tmp, 'x.json'), d=copy.deepcopy(doc))
        cases.append({'name': '%s%s' % (name, ' (infinite cargo)' if cargo else ''), 'seed': [k for k, v in SEEDS.items() if v is s][0],
                      'infinite_cargo': cargo, 'before': doc, 'after': json.loads(p._text()), 'dirty': p.dirty})
SET.configure(path=False)
path = os.path.join(tmp, 'new', 'profile.json')
p = PP.Profile(seed=seed, path=path)
created = json.loads(open(path, encoding='utf8').read())['created']
new_file = open(path, encoding='utf8').read().replace(created, CREATED)
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, 'w', encoding='utf8') as fh:
    json.dump({'seeds': SEEDS, 'created': CREATED, 'cases': cases, 'new_file': new_file}, fh, ensure_ascii=False,
              separators=(',', ':'))
print('%d cases' % len(cases))
