"""Writes what OpenOrbit's Profile.generate produces for each reference profile (crates/lo-save/tests/fixtures/*.json), for
LibreOrbit's tests (crates/lo-save/tests/markers.rs). The profile is loaded from its document (no file is written).

Usage: python3 -I gen_markers.py <OpenOrbit checkout> <fixtures folder>
"""
import json, os, sys

oo, fixtures = (os.path.abspath(a) for a in sys.argv[1:3])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
import player_profile as PP  # noqa: E402

MARKERS = ('@ayarlar', '@tuslar', '@madenler', '@formasyon', '@giris_yaniti')
seed = json.load(open(os.path.join(oo, 'server', 'data', 'defaults', 'login.json'), encoding='utf8')).get('seed', {})
out = {}
for name in sorted(os.listdir(fixtures)):
    if not name.endswith('.json'):
        continue
    d = json.load(open(os.path.join(fixtures, name), encoding='utf8'))
    p = PP.Profile(seed=seed, path=os.path.join(fixtures, name), d=d)
    out[name] = {m: list(p.generate(m)) for m in MARKERS}
os.makedirs(os.path.join(fixtures, 'expected'), exist_ok=True)
with open(os.path.join(fixtures, 'expected', 'markers.json'), 'w', encoding='utf8') as fh:
    json.dump(out, fh, ensure_ascii=False, indent=1)
print('%d profiles x %d markers' % (len(out), len(MARKERS)))
