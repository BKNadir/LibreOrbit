"""Generates protocol test cases with OpenOrbit's reference implementation (server/protocol.py).

For every message class: random field values are encoded and decoded by protocol.py; LibreOrbit's
codec must give the same bytes and the same values (crates/lo-protocol/tests/differential.rs).
Random and mutated payloads check that decoding errors and partial results match too.

Usage (no dependencies; protocol.py only uses the standard library):
  python3 -I gen_cases.py <OpenOrbit/server> <out.jsonl> [--seed N] [--per-message K] [--fuzz M]
"""
import argparse, json, math, random, struct, sys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('server_dir')
    ap.add_argument('out')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--per-message', type=int, default=4)
    ap.add_argument('--fuzz', type=int, default=20000)
    a = ap.parse_args()
    sys.path.insert(0, a.server_dir)
    import protocol as P  # noqa: E402

    S = P.spec()
    rnd = random.Random(a.seed)
    classes = sorted(S.by_class)
    valid = {}

    def small_values(t, x):
        key = (t, json.dumps(x))
        if key not in valid:
            valid[key] = sorted(P._inverse_table(t, x))
        return valid[key]

    texts = ['', 'a', 'Hello', 'MDLuffy', 'été €', 'ünïcödé', '𝄞 emoji 🚀', 'x' * 300, 'tab\tnl\n']

    def gen_prim(t, x, depth):
        r = rnd.random()
        if t == 'int':
            if r < 0.03:
                return rnd.choice([1 << 40, -(1 << 40)])       # clamped (warning)
            return rnd.choice([0, 1, -1, 2 ** 31 - 1, -2 ** 31, rnd.randint(-2 ** 31, 2 ** 31 - 1), rnd.randint(-1000, 100000)])
        if t == 'uint':
            return rnd.choice([0, 1, 2 ** 32 - 1, rnd.randint(0, 2 ** 32 - 1)])
        if t in ('short', 'ushort', 'byte', 'sbyte'):
            return rnd.choice(small_values(t, x))
        if t == 'bool':
            return rnd.random() < 0.5
        if t == 'float':
            v = rnd.choice([0.0, 1.5, -2.25, rnd.uniform(-1e6, 1e6), rnd.uniform(-1, 1), float(rnd.randint(-100, 100))])
            return struct.unpack('>f', struct.pack('>f', v))[0]
        if t == 'double':
            return rnd.choice([0, 0.5, -1.25, rnd.uniform(-1e9, 1e9), rnd.randint(-2 ** 53, 2 ** 53), 1e20])
        if t == 'long':
            return rnd.choice([0, 1, -1, rnd.randint(-2 ** 53, 2 ** 53), rnd.randint(0, 10 ** 12)])
        if t == 'string':
            return rnd.choice(texts)
        if t == 'module':
            if depth >= 3 or r < 0.35:
                return None
            return gen_struct(rnd.choice(classes), depth + 1)
        raise ValueError(t)

    def gen_fields(steps, f, depth, n):
        for s in steps:
            t = s['t']
            if t in ('pid', 'super') and (t == 'pid' or s.get('id', True)):
                if rnd.random() < 0.08:
                    f.setdefault('_super', {})[str(n[0])] = rnd.randint(0, 65535)
                n[0] += 1
            if t == 'pid':
                continue
            if t == 'super':
                gen_fields(S.by_class[s['c']]['steps'], f, depth, n)
                continue
            name = s['f']
            if rnd.random() < 0.05:
                continue                                       # missing: the type's default
            if name.endswith('?') and rnd.random() < 0.5:
                name = name[:-1]                               # the name without '?' is accepted
            if t == 'list':
                e = s.get('e') or {'t': 'module'}
                k = rnd.choice([0, 0, 1, 2, 3])
                f[name] = [gen_prim(e['t'], e.get('x'), depth) for _ in range(k)]
            else:
                f[name] = gen_prim(t, s.get('x'), depth)

    def gen_struct(cls, depth, top=False):
        f = {} if top else {'_': cls}
        gen_fields(S.by_class[cls]['steps'], f, depth, [0])
        return f

    def js(v):
        """JSON-safe form: non-finite floats as {"__f": ...} (JSON cannot hold them)."""
        if isinstance(v, float) and not math.isfinite(v):
            return {'__f': 'nan' if v != v else ('inf' if v > 0 else '-inf')}
        if isinstance(v, dict):
            return {k: js(w) for k, w in v.items()}
        if isinstance(v, list):
            return [js(w) for w in v]
        return v

    P.warning = lambda m: None
    n_enc = n_fuzz = 0
    payloads = []
    with open(a.out, 'w', encoding='utf8') as out:
        for cls in classes:
            for _ in range(a.per_message):
                fields = gen_struct(cls, 0, top=True)
                case = {'kind': 'encode', 'name': cls, 'input': js(fields)}
                try:
                    payload = P.encode_message(cls, fields)
                except Exception as e:                         # noqa: BLE001 - any failure must also fail in Rust
                    case['encode_error'] = type(e).__name__
                else:
                    mid, name, dec, left, err = P.decode_message(payload)
                    case.update(payload=payload.hex(), decoded=js(dec), remaining=left, error=err is not None, fmt=P.fmt(dec, 300))
                    payloads.append(payload)
                out.write(json.dumps(case, ensure_ascii=False) + '\n')
                n_enc += 1
        ids = sorted(S.by_id)
        for i in range(a.fuzz):
            r = rnd.random()
            if r < 0.4 and payloads:                           # mutated valid payload
                p = bytearray(rnd.choice(payloads))
                for _ in range(rnd.randint(1, 4)):
                    if len(p) > 2:
                        p[rnd.randrange(2, len(p))] = rnd.randrange(256)
                if rnd.random() < 0.3:
                    p = p[:rnd.randint(0, len(p))]
                p = bytes(p)
            elif r < 0.9:                                      # a valid id and random bytes
                p = struct.pack('>H', rnd.choice(ids)) + bytes(rnd.randrange(256) for _ in range(rnd.randint(0, 48)))
            else:                                              # anything
                p = bytes(rnd.randrange(256) for _ in range(rnd.randint(0, 6)))
            mid, name, dec, left, err = P.decode_message(p)
            out.write(json.dumps({'kind': 'decode', 'payload': p.hex(), 'id': mid, 'name': name, 'decoded': js(dec),
                                  'remaining': left, 'error': err is not None, 'fmt': P.fmt(dec, 300)}, ensure_ascii=False) + '\n')
            n_fuzz += 1
    print('%d encode cases, %d decode cases -> %s' % (n_enc, n_fuzz, a.out))


if __name__ == '__main__':
    main()
