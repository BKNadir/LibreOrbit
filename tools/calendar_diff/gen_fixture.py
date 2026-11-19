"""Writes what OpenOrbit's event_calendar.py gives for the parts of the calendar that do not depend on the time zone
(intervals and section intervals in naive local time, holidays, seasons), for crates/lo-game/tests/calendar.rs.

Usage: python3 -I gen_fixture.py <OpenOrbit checkout> <output.json>
"""
import datetime, json, os, sys

oo, out_path = (os.path.abspath(a) for a in sys.argv[1:3])
sys.path.insert(0, os.path.join(oo, 'server'))
os.chdir(os.path.join(oo, 'server'))
import event_calendar as CAL  # noqa: E402

ivs = lambda v: [[a.isoformat(), b.isoformat()] for a, b in v]
WINDOWS = [('2022-12-25T00:00:00', '2023-02-10T00:00:00'), ('2026-06-01T00:00:00', '2028-03-01T00:00:00'), ('2026-10-30T13:15:00', '2027-01-07T08:00:00')]
out = {'windows': [], 'holidays': {}, 'seasons': {}}
for a, b in WINDOWS:
    x, y = datetime.datetime.fromisoformat(a), datetime.datetime.fromisoformat(b)
    w = {'a': a, 'b': b, 'events': {}, 'sections': {}}
    for k, e in CAL.EVENTS.items():
        w['events'][k] = ivs(CAL.intervals(k, x, y))
        secs = ([e['map']] if isinstance(e.get('map'), dict) else []) + e.get('map_extra', [])
        w['sections'][k] = [[ivs(CAL.section_intervals(s, x, y)), ivs(CAL.section_intervals(s, x, y, True))] for s in secs]
    out['windows'].append(w)
for y in range(2000, 2101):
    out['holidays'][str(y)] = [str(CAL._easter(y)), str(CAL._thanksgiving(y)), str(CAL._cny(y))]
for y in range(2020, 2040):
    for m in (1, 6, 11):
        t = datetime.datetime(y, m, 3, 12)
        out['seasons'][t.isoformat()] = CAL.season(t)
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, 'w', encoding='utf8') as fh:
    json.dump(out, fh, separators=(',', ':'))
print('%d windows, %d events' % (len(WINDOWS), len(CAL.EVENTS)))
