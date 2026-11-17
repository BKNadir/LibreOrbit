# Lancer LibreOrbit depuis le panneau d'OpenOrbit

Pendant le portage, le panneau d'OpenOrbit peut démarrer les services déjà écrits en Rust à la place de leurs
équivalents Python. Les arguments, les ports, les sauvegardes et les journaux restent les mêmes.

## Installation

1. Compiler LibreOrbit : `cargo build --release` (l'exécutable est `target/release/libreorbit(.exe)`).
2. Appliquer `processes.patch` à OpenOrbit (commit `5511c73` ou proche), depuis le dossier d'OpenOrbit :
   `git apply <chemin>/processes.patch`. Il ne modifie que `server/panel/processes.py`.
3. Désactiver la mise à jour automatique du panneau (« Only notify »), sinon elle remplace le fichier.
4. Indiquer l'exécutable au panneau, puis le redémarrer :
   ```
   setx LIBREORBIT_EXE C:\Users\<toi>\LibreOrbit\target\release\libreorbit.exe
   ```

Sans `LIBREORBIT_EXE`, le panneau fonctionne exactement comme avant.

## Services

`LIBREORBIT_SERVICES` choisit les services confiés à LibreOrbit (liste séparée par des virgules, `web` par défaut).
Aujourd'hui seul `web` existe ; `game` viendra avec la suite de l'étape 2.

## Retour en arrière

Supprimer la variable (`reg delete HKCU\Environment /v LIBREORBIT_EXE /f`) puis redémarrer le panneau, ou annuler
le correctif : `git apply -R <chemin>/processes.patch`.
