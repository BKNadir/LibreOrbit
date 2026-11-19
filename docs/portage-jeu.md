# Portage du serveur de jeu (`game.py` → `lo-game`)

## Où on en est

| Partie | État |
| --- | --- |
| Écoute du port 8081, tramage, journal `gs.log` (même format, clé de session masquée) | ✅ identique à `game.py` |
| `VersionRequest` → `VersionCommand`, `KeepAlive` → `StayinAlive` | ✅ identique |
| Sauvegarde (`lo-save`) : lecture, nouveau profil, écriture identique à OpenOrbit | ✅ (profils réécrits octet pour octet) |
| Marqueurs de connexion qui ne dépendent que de la sauvegarde : `@ayarlar`, `@tuslar`, `@madenler`, `@formasyon`, `@giris_yaniti` | ✅ (identiques à `Profile.generate`, ordre des champs compris) |
| Marqueurs `@gemi`, `@arayuz`, `@isaret` (`lo_game::login_markers` : anneaux des portails, console GM, mode admin, fenêtres d'événement selon le calendrier, barres déplacées, robot de réparation, compétence Ingénierie, zone de paix) | ✅ identiques (`crates/lo-game/tests/login_markers.rs`) |
| Sauvegarde : compléments des anciens profils (`Profile::migrate`, `Profile::load` = le `Profile(...)` d'OpenOrbit) | ✅ identiques (`crates/lo-save/tests/migrate.rs`, 16 cas de `gen_migrate.py`, document et ordre des clés compris) |
| Cartes et portails (`map_links.py`, `map_layout.py` → `lo_game::map_links`, `map_layout`) et le `random` de Python (`lo_core::pyrandom`) | ✅ identiques (`tools/map_diff`) |
| `patch_template` (modèles de connexion adaptés au profil) | à faire (prochaine étape) |
| Calendrier des événements (`event_calendar.py`, avec `event_appearances.py` et la partie événements de `settings.py`) | ✅ identique (`tools/calendar_diff`, voir ci-dessous) |
| Connexion (`LoginRequest`) et chargement de la carte (`ReadyRequest`) | à faire (ci-dessous) |
| Simulation de la carte, combat, systèmes du jeu | à faire |

Vérification : `tools/game_diff/compare.py` lance `game.py` puis LibreOrbit avec une copie de la même sauvegarde,
fait jouer le même client scripté (`lo-probe`) et compare les messages reçus et `gs.log`. Aujourd'hui :
`version,keepalive,version` → aucune différence.

## Marqueurs `@gemi`, `@arayuz`, `@isaret`

`lo_game::login_markers` produit les trois marqueurs qui dépendent d'autres systèmes. Ce que la session liera plus tard est
passé en argument : énergie du N.O.V.A. (`captain_energy`), visibilités propres à la session (`window_hooks`), joueur
invité ou hôte, station proche. Comme dans OpenOrbit, lire le niveau d'une compétence ajoute un enregistrement
`skills` vide à un profil qui n'en a pas.

Vérification : `tools/save_diff/gen_login_markers.py` fait produire les marqueurs par `Profile.generate` pour les profils
de référence et des variantes (portails terminés, compétence au maximum ou non, P.E.T., barres déplacées avec des ordres
valides et invalides, robot sur un drone, portail, positions décimales), sous 5 réglages du panneau (console GM, mode admin
et vaisseau admin), en invité ou non, avec 5 stations (dont une pile au bord de la zone de paix) et des balises en jeu.
`@arayuz` est demandé à 12 instants, avec et sans visibilités de session, en ne gardant que les instants qui donnent la même
interface dans 6 fuseaux (de -11 h à +14 h) : le test ne dépend donc pas du fuseau de la machine. Résultat : 135 cas,
aucune différence. Deux erreurs volontaires (visibilités de session ignorées, bord de la zone de paix) sont détectées.

## Cartes et portails

`lo_game::map_links` lit les fichiers du client (`spacemap-config.xml`, `maps-config.xml`) comme OpenOrbit : noms,
niveaux requis, échelles, portails (positions sur la grille, portails ajoutés : 5-4, LoW), mondes des cartes absentes du
modèle (station déplacée et tournée, avant-postes, structures, donneurs de quêtes), bulles des portails et des stations.
`lo_game::map_layout` reprend `map_layout.json` (obstacles, zones, zones sûres, champs de mines, déplacement des aliens
autour des obstacles par A*). Le tirage au hasard passe par `lo_core::pyrandom`, le Mersenne Twister de Python : avec la
même graine, LibreOrbit tire les mêmes nombres qu'OpenOrbit, ce qui permet de comparer exactement les fonctions aléatoires.

Vérification : `tools/map_diff/compare.py` (questions identiques à `map_links.py` / `map_layout.py` et à l'exemple
`map_query`) : tables lues, propriétés de chaque carte pour chaque compagnie, `map_world` de toutes les cartes, point
d'arrivée pour chaque couple de cartes, bulles, 300 points et 120 segments par carte (dont les chemins A*), tirages au
hasard avec la même graine, dans 3 scénarios (standard, sans limite de niveau, monde du World Editor qui modifie
`map_layout.json`). Résultat : 411 660 valeurs, aucune différence. Deux erreurs volontaires (rotation des avant-postes,
départage des égalités de l'A*) sont détectées.

## Calendrier des événements

`lo_game::event_calendar` reprend `event_calendar.py` en entier : périodes annuelles, datées et uniques, fêtes (Pâques,
Thanksgiving, Nouvel An chinois, y compris le calcul par la nouvelle lune après 2040), rotation des saisons réelles après
`known_calendar`, événements hebdomadaires (`days`, `hours`, `nth_of_month`, `week_cycle`, `block_cycle`,
`week_pattern`), `together`, `counts_as`, sections de carte, conflits, `--event`, choix du panneau, `world_status`,
passes de combat (`PASS_RUNS`), `event_cmd`, `mood_cmds`, récompenses des portails, `intervals` et `upcoming` du
panneau. `lo_game::event_appearances` reprend `event_appearances.py`, `lo_core::settings` la lecture de `settings.json`
(choix des événements et interrupteurs utilisés) et `lo_core::pytime` l'heure locale « naïve » de Python
(`fromtimestamp`, `timestamp()`, `time.timezone`), dont dépend tout le calendrier.

Vérification : `tools/calendar_diff/compare.py` pose les mêmes questions à `event_calendar.py` et à LibreOrbit (exemple
`calendar_query`), dans le même fuseau horaire, pour 7 scénarios (standard, choix du panneau, `--event all`,
`--event none` + noms, conflits forcés + passes, passes, monde du World Editor qui modifie `events.json`) et 4 fuseaux
(Europe/Brussels, America/New_York, Australia/Lord_Howe avec son heure d'été d'une demi-heure, UTC). Instants : au
hasard de 2022 à 2035, une grille sur 2025-2027, les bords de chaque période (trouvés par OpenOrbit lui-même, testés une
seconde avant, pile et une demi-seconde après) et les changements d'heure. Pour chaque instant et chaque événement :
`period`, `_on`, `is_active`, `forced`, `time_left`, `news`, les fenêtres des sections ; puis `active`, `event_cmd`,
`mood_cmds`, `has_type`, succès, récompenses des portails ; plus `intervals`, `section_intervals`, `upcoming`, les fêtes
de 1990 à 2100, les saisons et les données chargées. Résultat : 185 millions de valeurs comparées
(28 combinaisons, environ 6 900 instants chacune), **aucune différence**. Une erreur volontaire (seuil de maturité des
saisons) est bien détectée. La partie indépendante du fuseau (intervalles, fêtes, saisons) est aussi un test Rust
(`crates/lo-game/tests/calendar.rs`).

Écarts volontaires, tous sur des données invalides (refusées au chargement ou ignorées là où Python s'arrête sur une
erreur) : fin de période illisible, 29 février d'une année non bissextile, boucle `counts_as` / `together`.
Sous Windows, l'heure locale passe par `chrono` (Python utilise la bibliothèque C de Windows) : mêmes réglages du
système, mais les cas limites des années passées aux règles différentes n'ont pas été comparés.

Le calendrier n'est pas encore branché sur les sessions : il le sera avec `LoginRequest` (`@arayuz`, `EventUpdateCommand`).

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
