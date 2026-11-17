# Architecture et plan de portage

## Principe de la phase 1 : « la même chose qu'OpenOrbit, en Rust »

1. **Compatibilité totale des fichiers.** LibreOrbit lit et écrit les mêmes fichiers qu'OpenOrbit :
   sauvegardes (`profile.json` par emplacement), réglages, données du jeu (`data/*.json`), modèles de
   connexion (`login.json`, `maps.json`, `responses.json`). Une sauvegarde passe de l'un à l'autre sans conversion.
2. **Même interface.** Le panneau garde exactement la même interface web (`server/panel/ui`).
   Le serveur Rust expose les mêmes ports et accepte les mêmes arguments que `web.py` et `game.py`.
3. **Vérification par comparaison.** Chaque système porté est comparé au code Python d'OpenOrbit,
   qui sert de référence de comportement (le dépôt d'origine n'a pas de tests).

Les améliorations (actions en lot de l'éditeur, serveur commun, sauvegardes officielles, transferts,
mode créatif) viennent ensuite, sur une base dont le comportement est déjà vérifié.

## Composants d'OpenOrbit et leur équivalent

| OpenOrbit (Python) | Rôle | Ports | LibreOrbit |
| --- | --- | --- | --- |
| `protocol.py` | protocole du client | | `lo-protocol` ✅ |
| `web.py` | connexion web du client (links.php, login.php, maps.php), fichiers CDN | 8090, 80 (+843, 9339, 8082) | `lo-web` ✅ |
| `paths.py`, `game_data.py`, `platform_compat.py` | emplacements, données avec les patchs du World Editor | | `lo-core` ✅ |
| `game.py` | sessions, connexion, paquets de bienvenue, routage des requêtes | 8081 | `lo-game` |
| `world.py` et une centaine de modules | simulation des cartes, combat, systèmes du jeu | | `lo-game` (un module par système) |
| `chat.py` | tchat (protocole « AChat » / Infinicast) | 7771 | `lo-chat` |
| `player_profile.py`, `save_*.py` | sauvegarde du joueur | | `lo-save` |
| `legacy_protocol.py` | ancien protocole texte (« 0\|LM\|… ») encore utilisé par le client | | `lo-protocol` |
| `panel/` (≈10 000 lignes) | panneau : processus, client, Wine, éditeur, mondes, amis | 8077 | `lo-panel` (même interface web) |

Le tout sera livré comme un seul exécutable (`libreorbit`), avec des sous-commandes pour chaque service.

## Organisation du dépôt

```
crates/
  lo-core/       emplacements, données + mondes, JSON façon Python, logs ✅
  lo-protocol/   protocole du client                                   ✅
  lo-save/       sauvegardes (même format que profile.json d'OpenOrbit) à venir
  lo-web/        connexion web du client (8090, 80)                    ✅
  lo-chat/       tchat (7771)                                          à venir
  lo-game/       sessions, cartes, systèmes du jeu (8081)              à venir
  lo-panel/      panneau : API HTTP + service d'une interface          à venir
  libreorbit/    l'exécutable qui assemble le tout (service `web`)     ✅
frontends/
  openorbit/     interface d'OpenOrbit, copiée telle quelle            ✅
  <nouvelle>/    seconde interface, plus tard
data/            données du jeu reprises d'OpenOrbit
docs/            architecture, décisions, API du panneau
tools/           outils de développement (comparaison avec OpenOrbit)
```

Règles de dépendance : `lo-core` et `lo-protocol` ne dépendent de rien d'autre ; `lo-save` seulement de `lo-core` ; `lo-game`, `lo-web`
et `lo-chat` utilisent les deux premiers ; `lo-panel` utilise `lo-save` et pilote les services ; seul
l'exécutable `libreorbit` les assemble. Le panneau ne connaît les interfaces que comme des dossiers de
fichiers statiques : l'API est le seul contrat entre les deux ([`panel-api.md`](panel-api.md),
136 points d'accès).

## Séquence de connexion du client (étape 2)

D'après `game.py`, `web.py` et `chat.py` :

1. Le client est lancé avec l'adresse `http://127.0.0.1:8090/#darkorbit://`. La page web de connexion
   (8090) le redirige vers le jeu ; l'API web du client (port 80) répond à `links.php`, `login.php`
   (identifiant, vaisseau, carte, langue, 2D tirés de la sauvegarde) et `maps.php`.
2. Le client ouvre la connexion de jeu (8081). Premier paquet : `VersionRequest`, avec une longueur sur 4 octets.
3. `LoginRequest` : le serveur envoie le paquet « entry » de `login.json` (266 messages). Les marqueurs
   `@…` de ce modèle sont produits à partir de la sauvegarde (vaisseau, réglages, raccourcis, interface…).
4. `ReadyRequest` : le serveur envoie le paquet « ready » (35 messages) et démarre la simulation de la carte.
5. Le client se connecte au tchat (7771) avec l'adresse reçue dans `ChatParamCommand`.

Premier objectif concret : le client arrive sur la carte avec son vaisseau, sans aliens ni combat.
Ce qu'il faut pour ça : `lo-web`, la session de `game.py`, les modèles de connexion et la production
des marqueurs `@…` depuis la sauvegarde.

## Transition pour le panneau

Pour garder l'interface identique dès le début, deux étapes :

1. Le panneau Python d'OpenOrbit lance l'exécutable LibreOrbit au lieu de `web.py` et `game.py`, service par
   service ([`tools/openorbit_bridge`](../tools/openorbit_bridge/README.md) : un petit correctif de
   `panel/processes.py` et la variable `LIBREORBIT_EXE`). Cela permet de tester le serveur Rust dans les vraies
   conditions sans attendre le portage du panneau.
2. Le panneau est porté en Rust (carte 14) : il sert `frontends/openorbit/` et la même API HTTP,
   et le Python n'est plus nécessaire. Une seconde interface pourra ensuite être ajoutée dans `frontends/`.

## Serveur web (`lo-web`)

* Mêmes routes, mêmes réponses et mêmes journaux que `web.py` (sessions multi-clients, langue de l'écran de
  connexion, guides des portails et de Galaxy Trek avec les patchs du monde choisi, fichiers CDN et
  téléchargement des images du jeu à la première demande, ports TCP 843/9339/8082 seulement journalisés).
* Vérification : `tools/web_diff/compare.py` lance `web.py` puis LibreOrbit sur les vrais ports avec une copie de
  la même sauvegarde, envoie 38 requêtes et compare statut, en-têtes, corps, `http.log`, `unknown.log`, `tcp.log`
  et `web_settings.json`, dans le monde standard et avec un monde du World Editor : aucune différence.
* Écart volontaire : une réponse 204 (préflight CORS) n'a pas d'en-tête `Content-Length`, comme l'exige HTTP ;
  `web.py` envoie `Content-Length: 0`. Les navigateurs acceptent les deux.
* Pas repris : la migration des sauvegardes de l'ancien format v1 (`save_migration.py`), faite par le panneau.

## Protocole (`lo-protocol`)

* La description des messages vient de `data/protocol_read.json` (extraite du client par l'outil
  d'OpenOrbit). Les corrections manuelles d'OpenOrbit (`FIXES`) sont reprises.
* Les valeurs sont dynamiques (`Struct`, `Value`), comme les dictionnaires du code Python, pour que le
  portage suive le code d'origine. Des types Rust générés pour les messages les plus utilisés pourront
  venir plus tard.
* Vérification : `tests/differential.rs` compare à `protocol.py` les octets produits, les valeurs
  décodées, les octets restants et les erreurs. Jeu enregistré : 2 748 messages (deux par classe) et
  6 000 décodages de données aléatoires ou abîmées. Vérifié aussi sur 16 488 messages et 200 000 décodages.
* Écarts volontaires avec le Python, tous sur des entrées invalides : profondeur d'imbrication limitée
  à 64 au lieu de la limite de récursion de Python, et trame de plus de 16 Mio refusée.
