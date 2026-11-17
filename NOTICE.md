# Notices

## License

LibreOrbit's own work is licensed under the PolyForm Noncommercial License 1.0.0 (`LICENSE`).
Material from OpenOrbit keeps its MIT license (`LICENSES/MIT-OpenOrbit.txt`): the files listed below
and the parts of the code ported from it.

## OpenOrbit

LibreOrbit is a Rust reimplementation inspired by and partly derived from
[OpenOrbit](https://github.com/kaanumay/OpenOrbit) by Kaan UMAY (MIT license).
The behaviour of the server, the protocol description and the game data files
follow OpenOrbit as of commit `5511c73783b4cd3d2b298e9bdd680a02951667db`
(2026-11-07).

## Files under `data/`

| File | Origin |
| --- | --- |
| `protocol_read.json` | MIT (OpenOrbit). Copied unchanged from OpenOrbit (`server/data/`). OpenOrbit extracts it from the DarkOrbit Unity client 1.1.115 (unchanged in 1.1.116 and 1.1.117). SHA-256 `78336c818fa17aca4645c01d5a1bed9601044cd6fa56db92d04c08c11449384c`. |

## Files under `frontends/openorbit/`

MIT (OpenOrbit). OpenOrbit's admin panel interface (`server/panel/ui` at the commit above), copied unchanged: HTML,
JavaScript, CSS, translations and images, by Kaan UMAY (MIT license). Some images show DarkOrbit
elements (company logos, ships): see below.

## DarkOrbit

DarkOrbit, its client, graphics, sounds and texts belong to Bigpoint GmbH.
LibreOrbit is not affiliated with or endorsed by Bigpoint. LibreOrbit does not
include or distribute the game client; each player downloads it from Bigpoint.
Some data files describe the game (protocol, items, maps): check their origin
before publishing them.
