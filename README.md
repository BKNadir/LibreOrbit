# LibreOrbit

Serveur DarkOrbit libre, écrit en Rust, pour le client Unity original. LibreOrbit est une
réécriture inspirée d'[OpenOrbit](https://github.com/kaanumay/OpenOrbit) (Kaan UMAY, licence MIT),
avec ses propres objectifs : un serveur commun léger (jusqu'à un Raspberry Pi), des sauvegardes
officielles ou modifiées, des transferts entre serveurs et un mode créatif admin journalisé.

> État : début du développement. Le protocole et le serveur web de connexion sont en place ; le serveur de jeu
> n'existe pas encore, le jeu n'est donc pas jouable avec LibreOrbit seul.

## Avancement

| Étape | État |
| --- | --- |
| 1. Protocole : 1 374 messages, décodage, encodage, tramage, compression | ✅ vérifié octet par octet contre OpenOrbit |
| 2. Connexion du client jusqu'à l'écran de jeu | en cours : serveur web ✅ (identique à `web.py`), serveur de jeu commencé ([plan](docs/portage-jeu.md)) |
| 3. Outil de comparaison avec le serveur d'OpenOrbit | à faire (le protocole a déjà le sien) |
| 4 à 15. Systèmes du jeu, panneau, éditeur | à faire (l'interface du panneau est déjà en place) |
| 16 à 19. Serveur commun, sauvegardes officielles, transferts, mode créatif | à faire |

Le plan complet et les choix techniques sont dans [`docs/architecture.md`](docs/architecture.md),
les décisions de conception dans [`docs/conception.md`](docs/conception.md).

## Organisation

```
crates/lo-core       emplacements, données du jeu et mondes, JSON façon Python, journaux
crates/lo-protocol   protocole du client (décodage, encodage, tramage)
crates/lo-web        serveur web de connexion (ports 8090 et 80)
crates/lo-game       serveur de jeu (port 8081), en cours
crates/lo-probe      client de test scripté, pour comparer les serveurs
crates/libreorbit    l'exécutable (`libreorbit web …`, `libreorbit game …`)
frontends/openorbit  interface du panneau d'OpenOrbit, copiée telle quelle
data/, cdn/          données et fichiers CDN repris d'OpenOrbit (voir NOTICE.md)
docs/                architecture, décisions, API du panneau
tools/               comparaisons avec OpenOrbit (protocole, serveur web, jeu, calendrier), lancement depuis son panneau
```

## Développement

Il faut [Rust](https://rustup.rs) 1.85 ou plus récent.

```sh
cargo test                     # tous les tests, dont la comparaison avec OpenOrbit (jeu de cas enregistré)
cargo clippy --all-targets
```

Comparaisons avec OpenOrbit (il faut Python 3 et une copie d'OpenOrbit) :

```sh
tools/protocol_diff/run.sh ../OpenOrbit                                              # protocole, cas aléatoires
sudo python3 -I tools/web_diff/compare.py ../OpenOrbit target/debug/libreorbit .     # serveur web (ports 80, 8090…)
python3 -I tools/game_diff/compare.py ../OpenOrbit target/debug/libreorbit target/debug/lo-probe .   # serveur de jeu
python3 -I tools/calendar_diff/compare.py ../OpenOrbit [--quick]                    # calendrier des événements
```

Pour essayer LibreOrbit dans le panneau d'OpenOrbit : [`tools/openorbit_bridge`](tools/openorbit_bridge/README.md).

## Licence et mentions

LibreOrbit est sous [PolyForm Noncommercial 1.0.0](LICENSE) : utilisation, copie, modification et partage
libres pour un usage personnel ou non commercial. Toute utilisation commerciale (vente, produit ou service
payant) demande l'accord écrit de BKNadir.

Les parties reprises d'OpenOrbit (Kaan UMAY) restent sous licence MIT
([`LICENSES/MIT-OpenOrbit.txt`](LICENSES/MIT-OpenOrbit.txt)) : l'interface `frontends/openorbit/`, les
données de `data/` et ce qui est porté de son code. DarkOrbit, son client, ses graphismes et ses textes
appartiennent à Bigpoint GmbH ; LibreOrbit n'y est pas affilié et ne distribue pas le client. Détails dans
[`NOTICE.md`](NOTICE.md).
