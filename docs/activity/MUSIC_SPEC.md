# Musique des lieux et des routes : cahier des charges

Demande du 22/11/2026 : dans l'activité, la playlist du lieu où se trouve le joueur se lance. Un admin donne un fichier audio ou un lien YouTube, et assigne chaque morceau à un ou plusieurs lieux (ou routes). Un bouton du panneau admin ouvre la page de configuration de la musique.

Branche `feature/music` (créée depuis `feature/map-import`, accord du 22/11/2026), PR en brouillon vers `feature/map-import`.

## 1. Décisions

| # | Sujet | Décision |
|---|---|---|
| M1 | Liens YouTube | Le son est **récupéré avec yt-dlp** et devient un fichier audio comme un autre (choix de l'utilisateur). Garde-fous ajoutés (§ 3.3) : désactivé par défaut (variable d'environnement), case « j'ai les droits sur cette musique », mêmes contrôles qu'un fichier envoyé. |
| M2 | En voyage | **Playlist des routes** : un morceau s'assigne à des lieux et/ou à des routes ; sur une route, on entend celle de la route. |
| M3 | Ordre | **Aléatoire, en boucle** : ordre mélangé, aucun morceau répété avant que tous soient passés. |
| M4 | Branche | `feature/music`, PR séparée. |

Risques signalés à l'utilisateur pour M1 (22/11/2026) : conditions d'utilisation de YouTube (pas de téléchargement) et règles des développeurs Discord (ne pas enfreindre celles d'un autre service) ; droits d'auteur portés par l'exploitant du bot, qui redistribue le morceau ; YouTube bloque souvent yt-dlp depuis des adresses de serveur (l'import peut échouer). YouTube n'est pas joignable depuis la session de développement : yt-dlp y est testé avec un faux exécutable ; il faut un essai réel.

## 2. Modèle

- `music_tracks` : `{ _id, universe_id, title, origin: { kind: "file" } | { kind: "youtube", video_id }, file_id (GridFS), mime, codec, duration_ms, bytes, places: [category_id], roads: [channel_id], created_by, created_at }`.
- Fichiers dans GridFS, bucket `music_files` (un morceau peut dépasser 16 Mo).
- Playlist d'un lieu = morceaux dont `places` contient le lieu ; d'une route = morceaux dont `roads` contient la route. Un morceau sans lieu ni route reste dans la bibliothèque de l'univers.
- Suppression d'un lieu ou d'une route (commandes, import E6) : retiré des assignations.

## 3. Sécurité

### 3.1 Fichiers envoyés (admins seulement, droits vérifiés côté serveur à chaque requête)
- Taille : 25 Mio par fichier ; **quota de 500 Mio par univers** ; durée max 15 min.
- **Signature** reconnue (MP3, OGG Vorbis/Opus, FLAC, WAV, M4A/AAC, WebM/Opus), puis **décodage complet** par Symphonia (Rust pur, pas de code natif) : un fichier qui ne se décode pas d'un bout à l'autre est refusé. Le type servi vient de ce décodage, jamais du nom ni du type annoncé par le client.
- Le nom d'origine n'est jamais réutilisé (titre saisi, nettoyé, 100 caractères max).
- Limitation de débit sur l'envoi.

### 3.2 Diffusion
- `GET /api/music/{id}` : session valide **dans l'univers du morceau**. En-têtes : type audio imposé, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: sandbox`, `Content-Disposition: inline` sans nom de fichier. Requêtes partielles (`Range`) pour l'avance rapide.
- Le jeton ne peut pas passer dans un `<audio src>` : le client télécharge le morceau avec son en-tête `Authorization` et le joue depuis une URL `blob:` locale (comme l'image de fond de carte).

### 3.3 yt-dlp (M1)
- Coupé par défaut : `MUSIC_YTDLP_ENABLED=true` pour l'activer ; binaire **en version figée** (somme de contrôle vérifiée dans l'image Docker).
- Seul un identifiant de vidéo YouTube est accepté (11 caractères `[A-Za-z0-9_-]`, extrait d'une URL youtube.com / youtu.be / music.youtube.com) ; l'URL passée à yt-dlp est reconstruite par nous.
- Lancé **sans shell**, avec des arguments fixes : pas de liste de lecture, pas de direct, durée max 15 min, taille max 25 Mio, meilleur flux audio seul (pas de ffmpeg), sortie dans un dossier temporaire à nous, délai max 3 min, une seule extraction à la fois par univers.
- Le fichier obtenu passe par les contrôles du § 3.1.
- L'admin coche « j'ai les droits sur cette musique » (enregistré avec le morceau).

## 4. Lecture dans l'activité
- La playlist suit la position du joueur (lieu, ou route en voyage) ; changement en fondu (2 s).
- Aléatoire en boucle (M3). Volume et coupure du son par joueur, gardés dans le navigateur.
- Navigateurs : sans geste de l'utilisateur, le son peut être bloqué ; un bouton « Activer le son » apparaît alors.
- Morceaux préchargés un par un (pas toute la playlist).

## 5. Découpage

| Étape | Contenu |
|---|---|
| S1 | Stockage et contrôle des fichiers : modèle, GridFS, validation (signature + décodage), quota. |
| S2 | API : envoi, liste, titre et assignations, suppression, diffusion avec `Range`, playlist de la position. |
| S3 | Lecteur dans l'activité : suit la position, aléatoire en boucle, fondu, volume. |
| S4 | Page admin « Musique » : bouton du panneau admin, bibliothèque, envoi, assignation aux lieux et routes, écoute. |
| S5 | yt-dlp : extraction encadrée (§ 3.3), image Docker. |

## 6. S1 livrée : stockage et contrôle des fichiers (22/11/2026)

- `src/music/audio.rs` : signature (MP3, OGG, FLAC, WAV, MP4/M4A, WebM) puis décodage complet par Symphonia 0.6 (+ `symphonia-opus`) ; refus d'un fichier avec une piste vidéo ou de sous-titres ; plus de 1 % de paquets illisibles = refus ; durée calculée sur les échantillons décodés (15 min max), taille 25 Mio max avant tout décodage.
- Fichiers de test (`tests/fixtures/audio/`, 2 s de son, générés par ffmpeg) : les 7 formats acceptés sont lus et chronométrés (± 100 ms) ; refusés : une page HTML nommée `.mp3`, un OGG corrompu, un FLAC réduit à son en-tête, une vidéo MP4, un faux OGG (signature seule).
- `src/database/music.rs` : collection `music_tracks`, bucket GridFS `music_files`, quota (500 Mio et 300 morceaux par univers), titres nettoyés (100 caractères), playlist d'un lieu ou d'une route, oubli d'un lieu ou d'une route, suppression du morceau et de son fichier. Test de bout en bout contre MongoDB (`a_track_lives_and_dies_in_mongodb`).
