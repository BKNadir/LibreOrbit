# API du panneau utilisée par l'interface OpenOrbit

Liste extraite de `frontends/openorbit/*.js` (appels `api('/api/...')`). `lo-panel` doit répondre à chacune avec le même format que `server/panel/panel.py` d'OpenOrbit pour que l'interface fonctionne sans modification.

| Point d'accès | Fichiers |
| --- | --- |
| `/api/about` | panel.js |
| `/api/backups` | panel.js |
| `/api/backups/buy` | panel.js |
| `/api/backups/delete` | panel.js |
| `/api/backups/new_save` | panel.js |
| `/api/backups/note` | panel.js |
| `/api/backups/restore` | panel.js |
| `/api/bot` | bots.js |
| `/api/bot/assign` | bots.js |
| `/api/bot/create` | bots.js |
| `/api/bot/delete` | bots.js |
| `/api/bot/detail` | bots.js |
| `/api/bot/llm` | bots.js |
| `/api/bot/llm/` | bots.js |
| `/api/bot/llm/models` | bots.js |
| `/api/bot/llm/provider` | bots.js |
| `/api/bot/llm/test` | bots.js |
| `/api/bot/name` | bots.js |
| `/api/bot/reset` | bots.js |
| `/api/bot/roster` | bots.js |
| `/api/bot/server_restart` | bots.js |
| `/api/bot/update` | bots.js |
| `/api/bots` | bots.js |
| `/api/calendar` | panel.js |
| `/api/client` | panel.js |
| `/api/client/auto` | panel.js |
| `/api/client/close` | panel.js |
| `/api/client/download` | panel.js |
| `/api/client/download_status` | panel.js |
| `/api/client/download_stop` | panel.js |
| `/api/client/select` | panel.js |
| `/api/client/use` | panel.js |
| `/api/close` | panel.js |
| `/api/discord` | panel.js |
| `/api/discord_rp` | panel.js |
| `/api/donate` | panel.js |
| `/api/editor` | saves.js |
| `/api/editor/apply` | saves.js |
| `/api/editor/catalog` | saves.js |
| `/api/editor/discard` | saves.js |
| `/api/editor/save` | saves.js |
| `/api/engine` | panel.js |
| `/api/engine/install` | panel.js |
| `/api/events` | panel.js |
| `/api/friend` | world_editor.js panel.js |
| `/api/friend/host` | panel.js |
| `/api/friend/join` | panel.js |
| `/api/friend/kick` | panel.js |
| `/api/friend/leave` | panel.js |
| `/api/friend/public_ip` | panel.js |
| `/api/friend/settings` | panel.js world_editor.js |
| `/api/friend/test` | panel.js |
| `/api/game/auction` | panel.js |
| `/api/game/auction/` | panel.js |
| `/api/game/auction/all` | panel.js |
| `/api/game/auction/auto` | panel.js |
| `/api/game/auction/bid` | panel.js |
| `/api/game/auction/events` | panel.js |
| `/api/game/auction/instant` | panel.js |
| `/api/game/buy` | panel.js |
| `/api/game/hangar` | panel.js |
| `/api/game/hangar/action` | panel.js |
| `/api/game/hangar/activate` | panel.js |
| `/api/game/hangar/design` | panel.js |
| `/api/game/hangar/move` | panel.js |
| `/api/game/shop` | panel.js |
| `/api/game/shop/subscribe` | panel.js |
| `/api/game/skylab` | panel.js |
| `/api/game/skylab/action` | panel.js |
| `/api/game/skylab/robot` | panel.js |
| `/api/item_info` | panel.js |
| `/api/languages` | panel.js |
| `/api/live` | panel.js |
| `/api/log` | panel.js |
| `/api/log/clear` | panel.js |
| `/api/multi/close` | panel.js |
| `/api/multi/close_all` | panel.js |
| `/api/multi/open` | panel.js |
| `/api/open_folder` | panel.js saves.js |
| `/api/panel_settings` | panel.js |
| `/api/patch` | panel.js |
| `/api/patch/install` | panel.js |
| `/api/pilot` | pilot.js |
| `/api/pilot/profile` | pilot.js |
| `/api/play` | panel.js saves.js |
| `/api/ports` | panel.js |
| `/api/ports/close` | panel.js |
| `/api/protocol` | panel.js |
| `/api/protocol/apply` | panel.js |
| `/api/protocol/generate` | panel.js |
| `/api/saves` | world_editor.js panel.js saves.js |
| `/api/saves/active` | panel.js saves.js |
| `/api/saves/copy` | saves.js |
| `/api/saves/delete` | saves.js |
| `/api/saves/empty_trash` | saves.js |
| `/api/saves/export` | saves.js |
| `/api/saves/import` | saves.js |
| `/api/saves/new` | panel.js |
| `/api/saves/note` | saves.js |
| `/api/saves/rename` | saves.js |
| `/api/saves/undo` | saves.js |
| `/api/saves/world` | world_editor.js |
| `/api/season_banners` | panel.js |
| `/api/seasons` | panel.js |
| `/api/server/restart` | panel.js |
| `/api/server/stop` | panel.js |
| `/api/settings` | panel.js |
| `/api/settings/copy` | panel.js |
| `/api/settings/delete_template` | panel.js |
| `/api/settings/presets` | panel.js |
| `/api/settings/reset` | panel.js |
| `/api/settings/save_template` | panel.js |
| `/api/shortcut` | panel.js |
| `/api/status` | panel.js |
| `/api/supporters` | panel.js |
| `/api/update` | panel.js |
| `/api/update/check` | panel.js |
| `/api/update/install` | panel.js |
| `/api/world` | world_editor.js |
| `/api/world/code` | world_editor.js |
| `/api/world/copy` | world_editor.js |
| `/api/world/create` | world_editor.js |
| `/api/world/delete` | world_editor.js |
| `/api/world/discard_draft` | world_editor.js |
| `/api/world/export` | world_editor.js |
| `/api/world/import` | world_editor.js |
| `/api/world/items` | world_editor.js |
| `/api/world/map` | world_editor.js |
| `/api/world/new_quest` | world_editor.js |
| `/api/world/patch` | world_editor.js |
| `/api/world/quest` | world_editor.js |
| `/api/world/section` | world_editor.js |
| `/api/world/server_restart` | world_editor.js |
| `/api/world/undo` | world_editor.js |
| `/api/world/update` | world_editor.js |
| `/api/worlds` | world_editor.js |

## Autres chemins

| Chemin | Rôle |
| --- | --- |
| `/` et les fichiers du dossier de l'interface | l'interface elle-même (`index.html`, `*.js`, `*.css`, `*.json`, `images/`) |
| `/game_images/<fichier>` | images et noms d'objets extraits du client par le greffon BepInEx (`dizin.json`, `adlar_<langue>.json`, pages d'atlas `.png`) |
