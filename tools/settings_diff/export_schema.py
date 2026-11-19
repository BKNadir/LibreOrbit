"""Writes OpenOrbit's settings schema (settings.py: SCHEMA with the sub-settings, GROUPS, PRESETS, RIVAL_LEVELS, UPPER_LIMIT, I32) to
crates/lo-core/src/settings_schema.json, which lo_core::settings embeds. Run it again when OpenOrbit's settings change.

Usage: python3 -I export_schema.py <OpenOrbit checkout> <output.json>
"""
import json, os, sys

oo, out = (os.path.abspath(a) for a in sys.argv[1:3])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
import settings as S  # noqa: E402

d = {'_source': 'OpenOrbit server/settings.py (MIT, see NOTICE.md), exported by tools/settings_diff/export_schema.py',
     'upper_limit': S.UPPER_LIMIT, 'i32': S.I32, 'groups': S.GROUPS, 'rival_levels': S.RIVAL_LEVELS, 'schema': S.SCHEMA,
     'presets': S.PRESETS}
with open(out, 'w', encoding='utf8') as fh:
    json.dump(d, fh, ensure_ascii=False, indent=1)
    fh.write('\n')
print('%d settings' % len(S.SCHEMA))
