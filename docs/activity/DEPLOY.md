# Déployer l'activité Discord VerseEngine

L'activité est servie par le binaire du bot (même conteneur, D1) sur le port 8787. Discord la charge dans une iframe via son proxy (`https://<client_id>.discordsays.com`). Le proxy envoie toutes les requêtes vers votre sous-domaine, où un reverse proxy TLS (ici Caddy) les transmet au bot.

```
Client Discord ──► <client_id>.discordsays.com (proxy Discord)
                       │  URL mapping « / » → activity.example.com
                       ▼
               Caddy (HTTPS, WebSocket) sur activity.example.com:443
                       │  http://127.0.0.1:8787
                       ▼
               Conteneur VerseEngine (bot + API + front)
```

Remplacez `activity.example.com` par votre sous-domaine dans toute la suite.

Vérifié le 18/11/2026 sur la documentation officielle (sources en fin de document).

## 1. Prérequis

- Un sous-domaine dont l'enregistrement DNS A/AAAA pointe vers la machine du bot, **avant** de lancer Caddy, pour qu'il puisse obtenir le certificat.
- Les ports 80 et 443 de cette machine joignables depuis Internet. Let's Encrypt valide le certificat par le port 80 ou 443.
- Docker et docker compose. Le compose fourni lance le bot et MongoDB en replica set.

## 2. Portail développeur Discord

Utilisez l'application **du bot** (D2) : https://discord.com/developers/applications.

| Page du portail | Réglage | Pourquoi |
|---|---|---|
| **OAuth2** → *Client information* | Copier le **Client ID** dans `DISCORD_CLIENT_ID` et le **Client Secret** dans `DISCORD_CLIENT_SECRET`. | Le serveur échange le code OAuth2 contre un jeton avec ce secret. Le secret ne doit jamais être versionné ni envoyé au navigateur. |
| **OAuth2** → *Redirects* | Ajouter `https://127.0.0.1`, puis enregistrer. | La doc l'exige, mais l'Embedded App SDK gère la redirection lui-même : cette valeur n'est jamais utilisée. |
| **Installation** → *Installation Contexts* | Garder **Guild Install** coché. | L'activité ne fonctionne que dans un serveur relié à un univers. Le serveur refuse les autres emplacements (`activity__guild_only`). |
| **Activities** → *Settings* | Cocher **Enable Activities**. | Discord crée alors la commande Entry Point « Launch ». Le bot la conserve quand il réenregistre ses commandes (`src/discord/commands_registration.rs`). |
| **Activities** → *URL Mappings* | Une seule ligne : **PREFIX** `/`, **TARGET** `activity.example.com`. | Voir ci-dessous. |

Règles de l'URL mapping (documentation Discord) :
- La cible s'écrit **sans protocole** : `activity.example.com`, pas `https://activity.example.com`. Le mapping sert à tous les protocoles, `https` comme `wss`.
- La cible doit désigner un répertoire, jamais un fichier comme `…/index.html`.
- Un seul mapping `/` suffit : le front, l'API `/api/*` et la WebSocket `/api/ws` passent tous par lui.
- Aucun mapping n'est nécessaire pour les images. `https://cdn.discordapp.com/attachments/`, `/avatars/` et `/icons/` sont des exceptions documentées de la CSP des activités. L'API ne renvoie que des URL de pièces jointes de ce CDN (`src/activity/images.rs`).

Le scope OAuth2 demandé par le front est `identify` uniquement. La guilde et les droits sont vérifiés par le bot (§ 6 d'`ARCHITECTURE.md`).

## 3. Configuration du conteneur

Dans le fichier `.env`, à côté de `docker-compose.yml` (jamais versionné) :

```dotenv
DISCORD_TOKEN=...                 # jeton du bot
MONGODB_USER=admin
MONGODB_PASSWORD=...
ACTIVITY_ENABLED=true
DISCORD_CLIENT_ID=123456789012345678
DISCORD_CLIENT_SECRET=...         # portail → OAuth2 → Client Secret
# ACTIVITY_SESSION_TTL_SECS=3600  # durée d'une session (60 à 86400 s)
```

- `DISCORD_CLIENT_ID` sert aussi d'argument de build (`VITE_DISCORD_CLIENT_ID`) : le front l'intègre à son JavaScript. Ce n'est pas un secret. Reconstruisez l'image si vous le changez.
- Dans le conteneur, l'API écoute sur `0.0.0.0:8787`. Le compose ne publie ce port que sur `127.0.0.1:8787` : seul un reverse proxy installé sur la machine hôte peut l'atteindre.

```bash
docker compose up -d --build
```

## 4. Reverse proxy Caddy

Caddy obtient et renouvelle seul le certificat HTTPS, et redirige HTTP vers HTTPS. Il relaie les WebSockets sans configuration particulière : `reverse_proxy` gère la mise à niveau HTTP et le tunnel bidirectionnel.

`/etc/caddy/Caddyfile`, Caddy installé sur la machine hôte :

```caddyfile
activity.example.com {
	encode zstd gzip

	# API, WebSocket /api/ws et front statique : tout est servi par le bot.
	reverse_proxy 127.0.0.1:8787 {
		# Un rechargement de la configuration de Caddy ne coupe pas immédiatement les
		# WebSockets ouvertes (évite que tous les clients se reconnectent en même temps).
		stream_close_delay 5m
	}

	# Pas de version de serveur dans les réponses.
	header -Server
}
```

```bash
sudo systemctl reload caddy
```

Remarques :
- Si Caddy tourne lui-même dans Docker, mettez-le sur le même réseau que le service `bot` et visez `bot:8787` au lieu de `127.0.0.1:8787`. Retirez alors la publication du port 8787 du compose.
- Avec un autre reverse proxy déjà en place (nginx, Traefik…), il doit : terminer le TLS, transmettre la mise à niveau WebSocket sur `/api/ws`, et garder une connexion inactive ouverte plus de 25 s (intervalle du battement de cœur du serveur).
- Les en-têtes de sécurité (`Content-Security-Policy` du front, `X-Content-Type-Options`, `Referrer-Policy`, `Cache-Control: no-store` sur l'API) sont posés par le bot : inutile de les dupliquer.
- Les fichiers JS et CSS du front ont des noms hachés à chaque build. Discord recommande ce cache-busting, car son proxy retire les en-têtes de cache des seules réponses `text/html`.

## 5. Vérifications

Depuis n'importe quel poste :

```bash
curl -sS -o /dev/null -w '%{http_code}\n' https://activity.example.com/api/i18n/fr
```

Résultat attendu : `200`.

```bash
curl -sS https://activity.example.com/api/me
```

Résultat attendu : `{"error":"activity__unauthorized"}` (HTTP 401).

Ensuite, dans Discord : rejoignez un salon vocal d'un serveur relié à un univers, ouvrez le lanceur d'applications et lancez l'activité. Logs du bot à surveiller :

- `activity: API démarrée` au démarrage ;
- `activity: change stream ouvert` pour chaque collection suivie ;
- `activity: session ouverte` à chaque connexion d'un joueur.

## 6. Limites connues

- Les sessions sont en mémoire : un redémarrage du bot oblige à se reconnecter (le front relance l'authentification silencieuse). Une seule instance du bot doit tourner, car les files de voyage, craft et loot sont aussi en mémoire.
- Le proxy Discord masque l'adresse IP des joueurs : la limitation de débit se fait par instance d'activité et par joueur, pas par IP.
- Discord conseille une IP de sortie fixe pour le serveur. Une IP dynamique peut avoir été bannie par Cloudflare à cause d'un précédent utilisateur, ce qui bloque les appels à l'API Discord pendant une heure.

## 7. Développement local

```bash
cloudflared tunnel --url http://localhost:5173
```

Pendant le développement, mettez temporairement l'URL du tunnel comme cible du mapping `/`. Lancez `npm run dev` dans `web/activity` : Vite proxifie `/api` vers `127.0.0.1:8787`. Discord rappelle qu'un domaine de tunnel qui ne vous appartient pas peut être réattribué : remettez le mapping de production ensuite.

## Sources

- Discord, [Building an Activity](https://docs.discord.com/developers/activities/building-an-activity) : redirect `https://127.0.0.1`, Client Secret, Enable Activities, mapping `/`.
- Discord, [Local Development › URL Mapping](https://docs.discord.com/developers/activities/development-guides/local-development#url-mapping) : format préfixe/cible, exceptions de la CSP (`cdn.discordapp.com/attachments/`…).
- Discord, [Networking](https://docs.discord.com/developers/activities/development-guides/networking) : proxy, URL `<client_id>.discordsays.com`, WebSocket seul transport temps réel pris en charge, ne pas faire confiance aux données du client.
- Discord, [Production Readiness](https://docs.discord.com/developers/activities/development-guides/production-readiness) : cache-busting, IP de sortie fixe.
- Discord, [Application Commands › Entry Point Commands](https://docs.discord.com/developers/interactions/application-commands#entry-point-commands) et [Bulk Overwrite Global Application Commands](https://docs.discord.com/developers/interactions/application-commands#bulk-overwrite-global-application-commands).
- Discord, [Get Application Activity Instance](https://docs.discord.com/developers/resources/application#get-application-activity-instance) ; [OAuth2](https://docs.discord.com/developers/topics/oauth2) (jeton : `application/x-www-form-urlencoded` uniquement).
- Caddy, [reverse_proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) (WebSocket, `stream_close_delay`) et [Automatic HTTPS](https://caddyserver.com/docs/automatic-https) (DNS avant le démarrage, ports 80/443).
