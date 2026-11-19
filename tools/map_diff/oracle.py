"""Answers map queries with OpenOrbit's map_links.py and map_layout.py (the reference for crates/lo-game/examples/map_query.rs).

Usage: python3 -I oracle.py <OpenOrbit checkout> <scenario.json> <queries.json>   (the World Editor world comes from OO_WORLD)
"""
import json, os, random, sys

oo, scenario, queries = (os.path.abspath(a) for a in sys.argv[1:4])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
sc = json.load(open(scenario, encoding='utf8'))
qs = json.load(open(queries, encoding='utf8'))

import settings as SET  # noqa: E402
SET.configure(path=sc.get('settings') or False)
import map_layout as ML  # noqa: E402
import map_links as H  # noqa: E402

tpl = json.load(open(os.path.join(oo, 'server', 'data', 'defaults', 'maps.json'), encoding='utf8')).get('5')
fl = lambda p: None if p is None else [float(v) for v in p]
out = []
for q in qs:
    op, m = q['op'], q.get('map')
    if op == 'tables':
        a = {'names': {str(k): v for k, v in sorted(H.NAMES.items())}, 'levels': {str(k): list(v) for k, v in sorted(H.LEVELS.items())},
             'scale': {str(k): v for k, v in sorted(H.SCALE.items())}, 'gates': {str(k): v for k, v in sorted(H.GATES.items())}}
    elif op == 'map':
        a = {'name': H.name(m), 'company': H.company(m), 'size': list(H.map_size(m)), 'class': H.map_class(m),
             'parked': H.parked(m), 'center': list(H.center(m)), 'client': H.client_map_name(m), 'dmz': H.giver_dmz(m),
             'levels': [H.level_required(m, f) for f in (None, 1, 2, 3)],
             'x8': [H.x8_home(f, lv, m) for f in (1, 2, 3) for lv in (11, 12)],
             'gates': [H.gates(m, f) for f in (None, 1, 2, 3)],
             'jumpgates': [[H.jumpgate_create(g, f)[1] for f in (None, 1, 2, 3)] for g in H.GATES.get(m, [])],
             'tips': [[H.gate_tip(g), H.gate_available(g, True)[1], H.gate_available(g, False)[1]] for g in H.GATES.get(m, [])],
             'layout': {'of': ML.of(m), 'walls': ML.walls(m), 'poi': ML.poi_msgs(m), 'one_way': ML.one_way_gates(m),
                        'structure': ML.structure_pos(m), 'asset': ML.structure_asset(m), 'lairs': ML.lairs(m), 'safe': ML.safe_zones(m),
                        'peace': ML.gate_peace_radius(m, 1000), 'navigates': ML.navigates(m),
                        'gate_pos': [ML.gate_pos(m, k) for k in q['keys']], 'arrival': [ML.arrival(m, k) for k in q['keys']],
                        'design_pos': [ML.design_pos(m, d) for d in range(0, 120)]}}
    elif op == 'world':
        a = []
        for f in (None, 1, 2, 3):
            H.PLAYER_FACTION = f
            a.append([H.map_world(m, tpl), H.map_world(m, None)])
        H.PLAYER_FACTION = None
    elif op == 'arrival':
        a = [H.arrival(q['src'], m, f) for f in (None, 1, 2, 3)]
    elif op == 'tip':
        a = [H.gate_tip(q['gate']), H.gate_available(q['gate'], True)[1]]
    elif op == 'asset':
        a = [H.asset_available(q['id'], q['tip'], near, q['wild'])[1] for near in (True, False)]
    elif op == 'points':
        a = [[ML.in_safe(m, x, y, mg), ML.blocked(m, x, y, pad)] for x, y, mg, pad in q['points']]
    elif op == 'segments':
        a = [[ML.clear(m, ax, ay, bx, by, pad), fl(ML.waypoint(m, *H.map_size(m), ax, ay, bx, by))] for ax, ay, bx, by, pad in q['segments']]
    elif op == 'random':
        random.seed(q['seed'])
        w, h = H.map_size(m)
        a = [ML.free_spot(m, (0, 0, w, h)), ML.free_spot(m, tuple(q['rect']), pad=q['pad'], tries=q['tries']),
             H.random_arrival(m), ML.mine_spots(m, H.gates(m)), ML.mine_spots(m, H.gates(m), n=3),
             [ML.mine_field_of(m, x, y) for x, y in q['probe']], random.random()]
    else:
        raise SystemExit('unknown op %s' % op)
    out.append(a)
print(json.dumps(out))
