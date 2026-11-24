# Musique des lieux et des routes : cahier des charges

Demande du 22/11/2026 : dans l'activité, la playlist du lieu où se trouve le joueur se lance. Un admin donne un fichier audio ou un lien YouTube, et assigne chaque morceau à un ou plusieurs lieux (ou routes). Un bouton du panneau admin ouvre la page de configuration de la musique.

Branche `feature/music` (créée depuis `feature/map-import`, accord du 22/11/2026), PR en brouillon vers `feature/map-import`.

## 1. Décisions

| # | Sujet | Décision |
|---|---|---|
| M1 | Liens YouTube | ~~Son récupéré avec yt-dlp~~ (22/11/2026, remplacé par M5). |
| M5 | Liens YouTube (révision du 22/11/2026) | **Rien n'est téléchargé** : la vidéo est lue par le **lecteur officiel de YouTube**, visible (200 × 200, exigence de YouTube). Chaque joueur peut cocher « Masquer » : seuls les fichiers audio envoyés par les admins sont alors joués (pas de pause). yt-dlp et Deno sont retirés. Raison : avec yt-dlp, le serveur copie la musique et contourne une protection de YouTube (risque juridique pour l'exploitant) ; avec le lecteur officiel, le son va directement de YouTube au joueur. |
| M2 | En voyage | **Playlist des routes** : un morceau s'assigne à des lieux et/ou à des routes ; sur une route, on entend celle de la route. |
| M3 | Ordre | **Aléatoire, en boucle** : ordre mélangé, aucun morceau répété avant que tous soient passés. |
| M4 | Branche | `feature/music`, PR séparée. |

Risques signalés à l'utilisateur pour M1 (22/11/2026) : conditions d'utilisation de YouTube (pas de téléchargement) et règles des développeurs Discord (ne pas enfreindre celles d'un autre service) ; droits d'auteur portés par l'exploitant du bot, qui redistribue le morceau ; YouTube bloque souvent yt-dlp depuis des adresses de serveur (l'import peut échouer). YouTube n'est pas joignable depuis la session de développement : yt-dlp y est testé avec un faux exécutable ; il faut un essai réel.

## 2. Modèle

- `music_tracks` : `{ _id, universe_id, title, origin: { kind: "file" } | { kind: "youtube", video_id }, file_id (GridFS), mime, codec, duration_ms, bytes, places: [category_id], roads: [channel_id], created_by, created_at }`.
- Fichiers dans GridFS, bucket `music_files` (un morceau peut dépasser 16 Mo).
- Playlist d'un lieu = morceaux dont `places` contient le lieu ; d'une route = morceaux dont `roads` contient la route. Un morceau sans lieu ni route reste dans la bibliothèque de l'univers.
- Suppression d'un lieu ou d'une route (import E6) : retiré des assignations dans la même transaction. Suppression d'un univers : ses morceaux et leurs fichiers.

## 3. Sécurité

### 3.1 Fichiers envoyés (admins seulement, droits vérifiés côté serveur à chaque requête)
- Taille : 25 Mio par fichier ; **quota de 500 Mio par univers** ; durée max 15 min.
- **Signature** reconnue (MP3, OGG Vorbis/Opus, FLAC, WAV, M4A/AAC, WebM/Opus), puis **décodage complet** par Symphonia (Rust pur, pas de code natif) : un fichier qui ne se décode pas d'un bout à l'autre est refusé. Le type servi vient de ce décodage, jamais du nom ni du type annoncé par le client.
- Le nom d'origine n'est jamais réutilisé (titre saisi, nettoyé, 100 caractères max).
- Limitation de débit sur l'envoi.

### 3.2 Diffusion
- `GET /api/music/{id}` : session valide **dans l'univers du morceau**. En-têtes : type audio imposé, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: sandbox`, `Content-Disposition: inline` sans nom de fichier.
- Le jeton ne peut pas passer dans un `<audio src>` : le client télécharge le morceau avec son en-tête `Authorization` et le joue depuis une URL `blob:` locale (comme l'image de fond de carte).

### 3.3 Vidéos YouTube (M5)
- Seul l'**identifiant de la vidéo** est gardé (11 caractères `[A-Za-z0-9_-]`, extrait d'une URL youtube.com / youtu.be / music.youtube.com ; tout le reste est refusé). Rien n'est téléchargé, copié ni stocké par le bot. Titre saisi par l'admin, case « j'ai les droits » obligatoire.
- Lecture dans l'activité par le lecteur officiel (`youtube-nocookie.com/embed`, `enablejsapi`), **visible, 200 × 200**, jamais recouvert ; piloté par messages (volume, fondus, fin de vidéo), sans charger de script de YouTube dans la page de l'activité.
- Dans Discord, le lecteur passe par le proxy de l'activité (URL mapping `/youtube` → `www.youtube-nocookie.com`, `DEPLOY.md`) : il aurait alors la même origine que l'activité. Il est donc enfermé dans un cadre `sandbox` **sans** `allow-same-origin` (aucun accès au jeton). En accès direct (développement), il garde l'origine de YouTube. CSP de l'activité : `frame-src 'self' https://www.youtube-nocookie.com`.
- « Masquer la vidéo » (par joueur, gardé dans le navigateur) : les vidéos sortent de la file, sans pause ; si l'endroit n'a que des vidéos, silence. Un lecteur qui ne se charge pas (15 s) est traité de même ; une vidéo en erreur (retirée, intégration interdite) est sautée.

## 4. Lecture dans l'activité
- La playlist suit la position du joueur (lieu, ou route en voyage) ; changement en fondu (2 s).
- Aléatoire en boucle (M3). Volume et coupure du son par joueur, gardés dans le navigateur.
- Navigateurs : sans geste de l'utilisateur, le son peut être bloqué ; un bouton « Activer le son » apparaît alors.
- Morceaux préchargés un par un (pas toute la playlist).

## 5. Découpage

| Étape | Contenu |
|---|---|
| S1 | Stockage et contrôle des fichiers : modèle, GridFS, validation (signature + décodage), quota. |
| S2 | API : envoi, liste, titre et assignations, suppression, diffusion, playlist de la position. |
| S3 | Lecteur dans l'activité : suit la position, aléatoire en boucle, fondu, volume. |
| S4 | Page admin « Musique » : bouton du panneau admin, bibliothèque, envoi, assignation aux lieux et routes, écoute. |
| S5 | ~~yt-dlp~~ → lecteur officiel de YouTube, visible, avec « Masquer » (§ 3.3, M5). |

## 6. S1 livrée : stockage et contrôle des fichiers (22/11/2026)

- `src/music/audio.rs` : signature (MP3, OGG, FLAC, WAV, MP4/M4A, WebM) puis décodage complet par Symphonia 0.6 (+ `symphonia-opus`) ; refus d'un fichier avec une piste vidéo ou de sous-titres ; plus de 1 % de paquets illisibles = refus ; durée calculée sur les échantillons décodés (15 min max), taille 25 Mio max avant tout décodage.
- Fichiers de test (`tests/fixtures/audio/`, 2 s de son, générés par ffmpeg) : les 7 formats acceptés sont lus et chronométrés (± 100 ms) ; refusés : une page HTML nommée `.mp3`, un OGG corrompu, un FLAC réduit à son en-tête, une vidéo MP4, un faux OGG (signature seule).
- `src/database/music.rs` : collection `music_tracks`, bucket GridFS `music_files`, quota (500 Mio et 300 morceaux par univers), titres nettoyés (100 caractères), playlist d'un lieu ou d'une route, oubli d'un lieu ou d'une route, suppression du morceau et de son fichier. Test de bout en bout contre MongoDB (`a_track_lives_and_dies_in_mongodb`).

## 7. S2 livrée : API (22/11/2026)

| Route | Qui | Rôle |
|---|---|---|
| `GET /api/music/here` | joueur | Endroit du joueur (lieu, ou route en voyage) et sa playlist. |
| `GET /api/music/tracks/{id}` | session de l'univers | Fichier : type contrôlé, `nosniff`, `sandbox`, sans nom de fichier ; un autre univers reçoit 404. |
| `GET /api/admin/music` | admin | Bibliothèque, place occupée et limites, lieux et routes de l'univers, import YouTube activé ou non. |
| `POST /api/admin/music/upload?title=…&rights=true` | admin | Envoi (corps brut, 25 Mio) ; contrôle hors de l'exécuteur async ; un envoi à la fois par univers. |
| `PUT /api/admin/music/{id}` | admin | Titre, lieux, routes (tous vérifiés dans l'univers). |
| `DELETE /api/admin/music/{id}` | admin | Morceau et fichier. |

- Admin **revérifié auprès de Discord** à chaque requête (une session qui se dit admin ne suffit pas) ; écritures limitées (3 d'affilée, puis 1 par seconde).
- Évènement temps réel `music_updated` (tout l'univers) après chaque changement : les lecteurs relisent leur playlist.
- Test de bout en bout contre MongoDB avec un faux Discord (`admins_build_the_library_and_players_hear_their_place`) : session falsifiée refusée (403), droits non cochés (400), page HTML et vidéo refusées (422), lieu d'un autre univers refusé, joueur qui entend la playlist de son lieu et reçoit le fichier avec les bons en-têtes, étranger refusé, suppression.

## 8. S3 livrée : lecteur dans l'activité (22/11/2026)

- `web/activity/src/music/` : `shuffle.ts` (aléatoire en boucle, jamais deux fois le même morceau d'affilée, y compris d'un tour à l'autre ; suit les changements de bibliothèque sans relancer le tour), `settings.ts` (volume et coupure, gardés dans ce navigateur seulement, lecture tolérante), `player.ts` (lecteur : fondu de 2 s au changement d'endroit, enchaînement, silence là où rien n'est prévu, morceau illisible sauté sans boucler, préchargement du suivant, URL `blob:` libérées, son bloqué par le navigateur → bouton « Activer la musique »).
- `MusicControl` : petite commande en haut à droite de la carte (titre, coupure, volume), absente tant qu'aucune musique n'est prévue. La playlist est relue au déplacement (`travel_updated`, actions de voyage, fin d'un trajet), au changement de bibliothèque (`music_updated`) et à la reconnexion.
- CSP de l'activité : `media-src 'self' blob:` ajouté (sinon le son téléchargé avec le jeton ne pourrait pas être lu).
- Vérifié dans Chromium (`music-harness.html`, page de développement absente du build ; `web/activity/e2e/music-harness.cjs`, avec la CSP réelle) : lecture depuis `blob:` (temps qui avance), fondu à l'entrée, bascule sur la musique de la route au déplacement, coupure gardée. Le cas « son bloqué » ne se produit pas dans Chromium sans écran : il est couvert par les tests du lecteur.
- **À vérifier dans le vrai client Discord** : que la CSP de Discord laisse jouer un son depuis `blob:` dans l'activité.

## 9. S4 livrée : page admin « Musique » (22/11/2026)

- Bouton « Musique » dans les actions du mode admin (à côté de « Carte de l'univers »), pour les administrateurs du serveur ; la page est chargée seulement quand elle s'ouvre.
- Envoi : fichier audio, titre proposé d'après le nom du fichier, case « j'ai les droits » obligatoire, limites rappelées ; le bouton dit pourquoi il est désactivé (fichier, taille, quota, titre, droits).
- Bibliothèque : place occupée, titre modifiable, durée et taille, écoute, assignation aux **lieux et routes** (recherche sans accents ni casse, 150 résultats affichés au plus pour les grandes cartes), enregistrement seulement s'il y a un changement, suppression en deux temps (pas de boîte de dialogue du navigateur, souvent bloquée dans Discord).
- Vérifié dans Chromium (`music-harness.html?view=admin`, `web/activity/e2e/music-admin-harness.cjs`) : envoi, recherche « kerival » qui trouve « Kérival » et la route « Kérival → Lorroc », enregistrement, écoute, suppression ; aucune erreur.

## 10. S5 révisée : lecteur officiel de YouTube (22/11/2026)

- Première version (yt-dlp, son téléchargé par le bot) **retirée** à la demande de l'utilisateur, après la question des droits d'auteur : `src/music/ytdlp.rs`, `docker/yt-dlp-requirements.txt`, Python, yt-dlp et Deno dans l'image, `MUSIC_YTDLP_*`.
- Serveur : `POST /api/admin/music/youtube` (`{url, title, rights}`) garde l'identifiant (`src/music/youtube.rs`) ; un morceau YouTube n'a pas de fichier (`file_id` absent, `/api/music/tracks/{id}` → 404) ; il compte dans le nombre de morceaux, pas dans la place occupée. La playlist envoyée au joueur précise `kind` (`file` ou `youtube`) et `video_id`.
- Client : `music/youtubeEmbed.ts` (adresse, `sandbox`, protocole de messages), `YouTubeFrame` (lecteur visible), moteur (`player.ts`) qui alterne fichiers et vidéos, respecte « Masquer » et se replie sur les fichiers si le lecteur ne se charge pas ; commande et lecteur empilés en haut à droite de la carte (rien ne recouvre le lecteur). Page admin : formulaire « Ajouter une vidéo YouTube ».
- Tests : moteur (vidéo affichée et volume transmis, « Masquer » sans pause, silence s'il n'y a que des vidéos, repli) ; protocole (adresse, `sandbox` sans `allow-same-origin` par le proxy, évènements) ; intégration MongoDB (ajout, lien refusé, pas de fichier, playlist mixte) ; Chromium avec la CSP réelle et un faux lecteur au même protocole : 200 × 200, commandes reçues, « Masquer », fichier qui prend le relais.
- **À vérifier dans le vrai client Discord** (impossible ici, YouTube injoignable) : que le lecteur officiel se charge par le proxy de Discord dans un cadre isolé. Sinon, l'activité joue les seuls fichiers audio, automatiquement.
