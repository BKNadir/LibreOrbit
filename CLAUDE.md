# LibreOrbit — guide for working on the project

Rust rewrite of OpenOrbit (https://github.com/kaanumay/OpenOrbit, Python, MIT), a server for the original DarkOrbit Unity
client. Phase 1: the same behaviour as OpenOrbit, then the owner's improvements (docs/conception.md).

## Rules

* Never push, publish or release without the owner's explicit "ok". Test locally first.
* Ask before anything risky (renaming release files, deleting, rewriting history).
* The owner writes in French: replies, README and docs in French; code, comments and commit messages in English.
* License: PolyForm Noncommercial 1.0.0 for LibreOrbit's own work; material from OpenOrbit keeps its MIT notice
  (LICENSES/MIT-OpenOrbit.txt, NOTICE.md).

## How porting is done

* Port one OpenOrbit module at a time, following docs/portage-jeu.md. The Python code is the reference of behaviour.
* Every ported piece is compared with OpenOrbit, not only unit-tested:
  * protocol: `tools/protocol_diff/run.sh <OpenOrbit>` (bytes, values, errors, log formatting);
  * web server: `tools/web_diff/compare.py` (real ports, same save, 38 requests, logs and written files);
  * game server: `tools/game_diff/compare.py` with `lo-probe` steps (messages received and gs.log);
  * saves: `crates/lo-save/tests` (profiles written by OpenOrbit are rewritten byte for byte).
* Files OpenOrbit writes (profiles, settings) and logs keep exactly the same format: `lo_core::pyjson` reproduces
  Python's `json.dumps`, `lo_protocol::display` its log formatting.
* Deliberate differences with OpenOrbit are listed in docs/architecture.md.
* `data/` and `cdn/` are unchanged copies of OpenOrbit's at the commit in NOTICE.md; refresh them as a whole.

## Layout

crates/lo-core (layout, data + worlds, Python-style JSON, logs), lo-protocol (client protocol), lo-save (profile),
lo-web (web login, done), lo-game (game server, in progress), lo-probe (scripted test client), libreorbit (the executable).
frontends/openorbit: OpenOrbit's panel interface, unchanged. tools/openorbit_bridge: run LibreOrbit from OpenOrbit's panel.

Checks before a commit: `cargo fmt --all`, `cargo clippy --all-targets` (no warnings), `cargo test`, and the comparison
tools for what was touched.
