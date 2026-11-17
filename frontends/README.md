# Interfaces du panneau

Le panneau d'administration (`lo-panel`, à venir) sert une interface web choisie parmi les dossiers
d'ici. Chaque interface est un dossier autonome de fichiers statiques qui parle au panneau par la même
API HTTP ; on peut donc en ajouter une sans toucher au serveur.

| Dossier | Contenu |
| --- | --- |
| `openorbit/` | l'interface d'OpenOrbit, copiée telle quelle (`server/panel/ui`, commit `5511c73`) |

## Règles

* Une interface ne contient que des fichiers statiques : `index.html` à la racine, puis ses scripts,
  styles, traductions et images.
* Elle n'utilise que l'API décrite dans [`docs/panel-api.md`](../docs/panel-api.md). Si une nouvelle
  interface a besoin d'un nouveau point d'accès, on l'ajoute à l'API, pas en contournant le panneau.
* `openorbit/` n'est pas modifiée : elle sert de référence de compatibilité. Les améliorations (actions en
  lot de l'éditeur, etc.) vont dans la nouvelle interface ou, pendant la transition, dans un correctif
  séparé et documenté.
