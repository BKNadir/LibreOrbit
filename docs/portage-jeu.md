# Portage du serveur de jeu (`game.py` → `lo-game`)

## Où on en est

| Partie | État |
| --- | --- |
| Écoute du port 8081, tramage, journal `gs.log` (même format, clé de session masquée) | ✅ identique à `game.py` |
| `VersionRequest` → `VersionCommand`, `KeepAlive` → `StayinAlive` | ✅ identique |
| Sauvegarde (`lo-save`) : lecture, nouveau profil, écriture identique à OpenOrbit | ✅ (profils réécrits octet pour octet) |
| Marqueurs de connexion qui ne dépendent que de la sauvegarde : `@ayarlar`, `@tuslar`, `@madenler`, `@formasyon`, `@giris_yaniti` | ✅ (identiques à `Profile.generate`, ordre des champs compris) |
| Marqueurs `@gemi`, `@arayuz`, `@isaret` (ils dépendent du calendrier, des portails, des équipements, des compétences) | à faire |
| Sauvegarde : compléments des anciens profils (`_migrate`), `patch_template` | à faire |
| Calendrier des événements (`event_calendar.py`) : prochaine étape, nécessaire à `@arayuz` et à la plupart des systèmes | à faire |
| Connexion (`LoginRequest`) et chargement de la carte (`ReadyRequest`) | à faire (ci-dessous) |
| Simulation de la carte, combat, systèmes du jeu | à faire |

Vérification : `tools/game_diff/compare.py` lance `game.py` puis LibreOrbit avec une copie de la même sauvegarde,
fait jouer le même client scripté (`lo-probe`) et compare les messages reçus et `gs.log`. Aujourd'hui :
`version,keepalive,version` → aucune différence.

## Ce que coûte la connexion

Relevé avec `lo-probe` sur `game.py` (sauvegarde neuve, carte 1-1) : **285 messages** à la connexion,
**61** au chargement de la carte, puis le monde (45 aliens, 20 boîtes, 80 minerais) et quelques messages de
succès, de récompense quotidienne et d'événement.

Les paquets viennent de `data/defaults/login.json` (enregistrement du vrai serveur, 266 + 35 messages). Ils sont :

* **recopiés** pour la plupart, après `Profile.patch_template` (vitesse, munitions, laboratoire…) ;
* **produits depuis la sauvegarde** pour 11 marqueurs : `@gemi` (vaisseau), `@giris_yaniti` (réponse de connexion),
  `@ayarlar` (réglages), `@tuslar` (touches), `@arayuz` (interface), `@madenler` (minerais), `@formasyon` (drones),
  `@isaret` (repères), `@harita_giris` / `@harita_hazir` (le monde de la carte : `data/maps.json` ou produit depuis
  `spacemap-config.xml`) ;
* **complétés par les systèmes** : 29 types de messages n'existent pas dans l'enregistrement (stations, portails, points
  d'intérêt, quêtes, inventaire, modules instables, Synthesis…).

Au chargement de la carte s'ajoutent les messages de connexion de la boutique, du P.E.T., des titres, des nouveaux
systèmes, des modules instables et des événements spéciaux, puis le démarrage du monde.

## Modules nécessaires, dans l'ordre

| Ordre | Module Python | Lignes | Rôle pour la connexion |
| --- | --- | --- | --- |
| 1 | `player_profile.py` | 1 282 | sauvegarde : chargement, migrations, marqueurs `@…`, `patch_template`, enregistrement |
| 2 | `map_links.py` | 451 | cartes et portails, monde des cartes non enregistrées |
| 3 | `settings.py` | 546 | réglages du jeu (fenêtres qui s'ouvrent seules, multiplicateurs) |
| 4 | `event_calendar.py` | 594 | événements en cours (décor des cartes, fenêtres) |
| 5 | `ranks.py`, `skill_tree.py`, `login_calendar.py` | 889 | rang du jour, compétences, calendrier de connexion |
| 6 | `quest.py` | 579 | quêtes jointes à la connexion |
| 7 | `shop.py`, `pet.py`, `ship_module.py` | 4 747 | messages de connexion de la boutique, du P.E.T., des modules |
| 8 | `achievement.py`, `season.py`, `battle_pass.py`, `daily_rewards.py` | 1 691 | titres, passes, récompenses |
| 9 | `new_systems.py`, `extra_events.py` | 265 | messages de connexion des systèmes récents |
| 10 | `world.py` | 4 642 | la carte vivante : aliens, boîtes, minerais, combat |
| 11 | `chat.py` | 613 | tchat (7771), démarré par le serveur de jeu |

Soit environ 17 000 lignes pour arriver sur une carte vivante avec le comportement exact d'OpenOrbit, avant les autres
systèmes du jeu.

## Méthode

1. Porter un module à la fois, en commençant par ce dont la connexion a besoin (tableau ci-dessus).
2. Après chaque module, rejouer la connexion avec `tools/game_diff/compare.py` : le nombre de différences doit baisser
   jusqu'à zéro pour l'étape `version,login,ready`.
3. Les valeurs qui dépendent du moment (heure, positions aléatoires des aliens et des minerais) sont masquées ou
   comparées par leur forme, pas par leur valeur.
