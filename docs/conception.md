# Décisions de conception

Décisions prises avec le porteur du projet (novembre 2026). « Proposé » signale ce qui reste à valider.

## Le projet

* Serveur Rust complet, inspiré d'OpenOrbit. Phase 1 : la même chose qu'OpenOrbit, avec la même
  interface ; les améliorations ensuite.
* Nom : LibreOrbit.
* Rien n'est publié en ligne sans accord explicite. Tests en local d'abord.

## Architecture

* Une carte du jeu = une unité isolée qui ne communique que par messages. Tout tourne sur une seule
  machine au début ; le passage à plusieurs machines devient un ajout, pas une réécriture.
* Serveur commun installable sur une petite machine toujours allumée (type Raspberry Pi), qui détient
  l'état partagé du monde et le tchat.

## Sauvegardes

* Toute sauvegarde naît officielle.
* Serveur aux règles officielles : elle le reste. Serveur aux règles modifiées : elle devient modifiée,
  définitivement, et ne peut plus rejoindre un serveur officiel. Une sauvegarde modifiée ne rejoint que les
  serveurs qui l'acceptent.
* Sur un serveur officiel, le serveur garde les données ; elles lui sont propres. Conséquences acceptées :
  une sauvegarde officielle ne se joue qu'en ligne sur son serveur, les sauvegardes automatiques côté serveur
  sont indispensables, « officiel » veut dire « je fais confiance à cet administrateur ».
* Serveurs modifiés : sauvegarde locale, éditeur et console admin libres.
* Proposé : le serveur calcule une empreinte de ses règles pour s'annoncer officiel ou modifié.

## Transferts entre serveurs

* Un protocole d'échange permet de transférer ou copier une sauvegarde depuis un autre serveur.
* Proposé : chaque serveur signe ce qu'il exporte ; l'administrateur tient la liste des serveurs dont il
  accepte les transferts.
* Proposé : officiel vers officiel = déplacement uniquement (pas de copie, pour éviter la duplication) ;
  officiel vers modifié = copie libre, la copie devient modifiée ; modifié vers officiel = jamais.
* Proposé : identité du joueur indépendante des serveurs (paire de clés générée chez lui).

## Mode créatif admin

* Sur un serveur officiel, les admins peuvent modifier leur équipement ou donner de l'uridium et des objets
  aux joueurs ; la sauvegarde reste officielle.
* Chaque intervention est tracée, avec la distinction entre « don » (ajout) et « modification » (changement
  de l'existant).
* Le journal est côté serveur. Le joueur peut le voir, pas le modifier. Personne, admins compris, ne peut
  effacer une ligne : une erreur se corrige par une nouvelle ligne.
* Proposé : trois états affichés (officielle, officielle avec dons, officielle avec modifications), le journal
  voyage avec la sauvegarde lors d'un transfert, chaque serveur choisit ce qu'il accepte.
* À décider : validation par un deuxième admin au-delà d'un certain montant.
