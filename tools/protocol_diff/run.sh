#!/usr/bin/env sh
# Compares LibreOrbit's protocol codec with OpenOrbit's protocol.py on a fresh random set of cases.
# Usage: tools/protocol_diff/run.sh <OpenOrbit checkout> [seed] [cases per message] [decode cases]
set -eu
openorbit=${1:?path to an OpenOrbit checkout}
seed=${2:-$(date +%s)}
per=${3:-6}
fuzz=${4:-50000}
root=$(cd "$(dirname "$0")/../.." && pwd)
out=$(mktemp -d)/protocol_cases.jsonl
python3 -I "$root/tools/protocol_diff/gen_cases.py" "$openorbit/server" "$out" --seed "$seed" --per-message "$per" --fuzz "$fuzz"
cd "$root"
LO_PROTOCOL_CASES="$out" cargo test -q -p lo-protocol --test differential -- --nocapture
echo "seed $seed: OK"
