# Import et génération de carte — cahier des charges

Rédigé le 19/11/2026 à partir des réponses de l'utilisateur. Sert de point de départ à une session dédiée. Complète `ARCHITECTURE.md` (décisions D1–D12) et `PROGRESS.md`.

## 1. Objectif

Dans la **vue admin** de l'activité, deux boutons :
- **« Importer une carte »** : import d'une sauvegarde Azgaar (`.map`), avec des instructions pour créer et sauvegarder une carte dans Azgaar ;
- **« Générer une map »** : carte simple générée par l'activité.

Dans les deux cas, on peut **valider** (double vérification) ou **annuler**. Après validation, les lieux, salons et routes sont **créés automatiquement**, avec les distances.

## 2. Décisions

| # | Sujet | Décision |
|---|-------|----------|
| I1 | Accès | Seuls le **créateur de l'univers** et les **administrateurs** voient et utilisent l'import et la génération. |
| I2 | Serveur sans univers | L'activité **doit se lancer** sur un serveur non relié à un univers. Les administrateurs du serveur peuvent alors créer l'univers par l'import ou la génération. |
| I3 | Azgaar dans Discord | On tente de faire tourner Azgaar dans l'activité (version figée, MIT, polices servies depuis notre domaine). **Si ce n'est pas possible** : redirection vers **azgaar.github.io**, puis import du `.map` sauvegardé. |
| I4 | Répartition sur plusieurs serveurs | **150 lieux au plus par serveur**, et respect des limites de Discord (§ 4). L'univers est **réparti sur plusieurs serveurs** en **minimisant les routes entre serveurs**. Avant la validation, l'admin voit **combien de serveurs** sont nécessaires, pour les créer. |
| I5 | Lieu | Comme `/place_create_place` (rôle + catégorie), **plus un salon textuel générique** valable pour tout type de lieu, par exemple `#place-centrale`. |
| I6 | États, provinces, régions d'Azgaar | **Ignorés** pour l'instant. |
| I7 | Routes | Une route par paire de lieux voisins, distance calculée sur le tracé Azgaar. **Routes maritimes : case « inclure »**, décochée par défaut. **Routes secrètes : un clic sur la route dans l'aperçu, puis une case « secrète »**. Les deux réglages sont indépendants. |
| I8 | Distances | **Aucune conversion ni multiplicateur** : la distance de la carte est conservée telle quelle, pour garder la cohérence spatiale. La durée des voyages se règle avec le modificateur de temps de l'univers et la vitesse des personnages. |
| I9 | Serveurs | Voir I4. Un menu permet d'indiquer les serveurs à utiliser. |
| I10 | Ré-import | **Ne rien recréer qui existe** : un lieu garde son bourg d'origine, et le ré-import met à jour positions et tracés, et ajoute ou retire ce qui a changé. |
| I11 | « Générer une map » | Une carte **simple**. Réglages : **nombre de lieux**, **graine affichée** (et regénération). Modifications possibles : **tracé des routes**, **ajout et suppression de routes et de lieux**, **renommage des lieux**. Pas de modification en profondeur (relief, biomes…). Rendu : la vue 3D détaillée actuelle. |
| I12 | Double vérification | Un écran récapitulatif, puis **la saisie du nom de l'univers**. |
| I13 | Progression et erreur | Barre de progression pendant la création. En cas d'erreur, **retour en arrière complet** (suppression de tout ce qui a été créé), avec sa propre barre de progression. |
| I14 | Univers absent | Mêmes informations que `/universe_create_universe` (nom, type d'installation), **plus la vitesse du temps en option**. Par défaut, temps réel (24 h par jour). |
| I15 | « Supprimer les lieux existants » | Les personnages présents sont déplacés vers un **lieu de départ** choisi dans la nouvelle carte. Les voyages en cours sont annulés. Les salons, catégories et rôles sont supprimés. Pour chaque **objet posé**, l'admin choisit de **le détruire** ou de **le rendre à son propriétaire, avec son contenu**. |
| I16 | « Créer en plus » | Les lieux existants dont le nom correspond à un bourg sont **rattachés** (déjà codé : `match_places`). Les autres bourgs retenus sont créés. |
| I17 | Brouillard | **Option propre à chaque univers**, au choix du créateur. |

## 3. Ce qui existe déjà (branche `feature/discord-activity`)

- Lecture du `.map` (`activity/map/azgaar_file.rs`, gzip / base64 / texte), testée sur une vraie sauvegarde (Moria, Azgaar 1.158) et comparée à son export JSON.
- Routes candidates entre bourgs voisins, avec leur **tracé** et une distance qui suit les bifurcations, sans les routes redondantes (`network.rs`, `prune_detours`). Zone explorable (`network::Zone`).
- Rattachement des bourgs aux lieux par nom (`azgaar::match_places`), relief (`resample_terrain`).
- `universe_maps` (positions, `road_paths`, zone, fond) ; fond PNG/JPEG/WebP (GridFS).
- Personnage qui suit le tracé des routes ; vue 3D détaillée ; vue 2D avec fond.

## 4. Contraintes de Discord (vérifiées dans la documentation officielle)

| Limite | Valeur | Source |
|--------|-------:|--------|
| Salons par serveur (catégories comprises) | 500 | code d'erreur 30013 |
| Rôles par serveur | 250 | code d'erreur 30005 |
| Salons par catégorie | 50 | type `GUILD_CATEGORY` |
| Création de serveur par un bot | **impossible** (plus de point d'accès « Create Guild ») | `resources/guild` |

L'admin doit donc **créer lui-même les serveurs** et y inviter le bot. L'import lui indique combien il en faut.

**Budget d'un serveur** avec le modèle actuel : un lieu = 1 rôle + 1 catégorie + 1 salon ; une route = 1 rôle + 1 salon, dans des catégories de routes d'au plus 50 salons. L'installation de base consomme aussi quelques rôles et salons.

Avec 150 lieux, il reste environ 95 rôles pour les routes (250 − 150 − environ 5 rôles de base). Or un réseau de 150 lieux compte généralement 1,3 à 2 routes par lieu, soit environ 200 à 300 routes. **C'est la limite des rôles qui bloque, pas celle de 150 lieux** (voir la question Q1).

## 5. Questions encore ouvertes

- **Q1. Rôles des routes** (voir § 4). Options :
  - (a) garder un rôle par route : l'import calcule le budget exact et place moins de lieux par serveur (environ 60 à 70) ;
  - (b) supprimer les rôles de route et donner l'accès au salon de la route par une permission individuelle sur le salon, posée au départ et retirée à l'arrivée : un serveur peut alors approcher 150 lieux, mais le mécanisme de voyage du bot change.
- **Q2. Objets posés sans propriétaire** (`owner_id` vide) : les détruire, ou les donner à un personnage ou un lieu choisi ?
- **Q3. Visuels si Azgaar ne tourne pas dans Discord** (I3) : le `.map` contient un SVG de la carte (ligne 5), mais Azgaar annonce sa suppression, et ses polices seraient approchées. Faut-il l'utiliser en attendant, ou afficher le rendu 3D/2D de l'activité ?
- **Q4. Brouillard** (I17) : les lieux jamais visités sont-ils cachés, ou affichés grisés et sans nom ? La carte est-elle assombrie hors des zones découvertes ?
- **Q5. Nom du salon générique** (I5) : `place-centrale` dans la langue du serveur (`central-square` en anglais) ?
