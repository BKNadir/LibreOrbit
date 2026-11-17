# LibreOrbit

Serveur DarkOrbit libre, écrit en Rust, pour le client Unity original. LibreOrbit est une
réécriture inspirée d'[OpenOrbit](https://github.com/kaanumay/OpenOrbit) (Kaan UMAY, licence MIT),
avec ses propres objectifs : un serveur commun léger (jusqu'à un Raspberry Pi), des sauvegardes
officielles ou modifiées, des transferts entre serveurs et un mode créatif admin journalisé.

> État : début du développement. Seul le protocole du jeu est en place. Le jeu n'est pas encore jouable.

## Avancement

| Étape | État |
| --- | --- |
| 1. Protocole : 1 374 messages, décodage, encodage, tramage, compression | ✅ vérifié octet par octet contre OpenOrbit |
| 2. Connexion du client jusqu'à l'écran de jeu | à faire |
| 3. Outil de comparaison avec le serveur d'OpenOrbit | à faire (le protocole a déjà le sien) |
| 4 à 15. Systèmes du jeu, panneau, éditeur | à faire (l'interface du panneau est déjà en place) |
| 16 à 19. Serveur commun, sauvegardes officielles, transferts, mode créatif | à faire |

Le plan complet et les choix techniques sont dans [`docs/architecture.md`](docs/architecture.md),
les décisions de conception dans [`docs/conception.md`](docs/conception.md).

## Organisation

```
crates/lo-protocol   protocole du client (décodage, encodage, tramage)
frontends/openorbit  interface du panneau d'OpenOrbit, copiée telle quelle
data/                données reprises d'OpenOrbit (voir NOTICE.md)
docs/                architecture, décisions, API du panneau
tools/protocol_diff  comparaison du protocole avec le code Python d'OpenOrbit
```

## Développement

Il faut [Rust](https://rustup.rs) 1.85 ou plus récent.

```sh
cargo test                     # tous les tests, dont la comparaison avec OpenOrbit (jeu de cas enregistré)
cargo clippy --all-targets
```

Pour comparer le protocole avec OpenOrbit sur un nouveau jeu de cas aléatoires (il faut Python 3
et une copie d'OpenOrbit) :

```sh
tools/protocol_diff/run.sh ../OpenOrbit
```

## Licence et mentions

MIT, voir [`LICENSE`](LICENSE). LibreOrbit reprend des éléments d'OpenOrbit et conserve sa mention de
copyright. DarkOrbit, son client, ses graphismes et ses textes appartiennent à Bigpoint GmbH ;
LibreOrbit n'y est pas affilié et ne distribue pas le client. Détails dans [`NOTICE.md`](NOTICE.md).
