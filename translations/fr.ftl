botname = VerseEngine

placeholder = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.
    .title = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.
    .message = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.

#Support
support_link = contact.cgbots@gmail.com ou @cgbots sur discord

tips = Soutient le projet
    .title = Soutient le projet
    .message = Merci de vouloir supporter le projet ! Tu peux faire le don au montant que tu souhaite à cette page: https://ko-fi.com/cgbot

start_message = Start Message
    .title = Merci d'utiliser {botname}
    .description = Pour commencer à utiliser le bot, commencez par créer un nouvel univers.
    Utilisez la commande `/{universe} {create_universe} [nom de votre univers] [type de setup]`
    Le type de setup détermine quels salons seront créés.
    Dans un setup partiel, seule la catégorie route et les rôles seront créés.
    Dans un setup complet, les catégories Admin, hors rp, rp et leurs selons sont également créés en plus.

carousel__previous_button = Précédent
carousel__next_button = Suivant
carousel__refresh_button = Actualiser

#Loot
loot = loot
    .description = Actions liées à la fouille de zones
loot_search = fouiller
    .description = Commencer à fouiller la zone actuelle
loot_stop = arrêter_fouille
    .description = Arrêter la fouille en cours

#Tables de loot
loot_table = loot_table
    .description = Gérer les tables de butin
loot_table_edit = modifier
    .description = Créer ou modifier une table de butin pour un salon
    .channel_id = id_salon
    .channel_id-description = ID du salon (Catégorie de lieu, salon de route, ou sous-salon de lieu)
    .rate_limit = temps_de_recharge
    .rate_limit-description = Temps de recharge en secondes entre deux loots
    .delay = délai
    .delay-description = Délai en secondes pour obtenir le loot
loot_table__modal_title = Éditeur de table de butin
loot_table__modal_field_name = Contenu de la table de butin
loot_table__modal_placeholder = # Guide de syntaxe des Tables de Butin
    - Les Tables de butins peuvent contenir deux types d'éléments. Les **items** et les **sets**.
    ## Items
    ```
    ­[nom de l'item]:[probabilité en %], [intervalle min-max], stock:[nombre], secret
    ```
    - Le nom des items sert à identifier les objets et est sensible à la casse.
    - La probabilité de chaque item est **absolue** et donnée en % de façon implicite (inutile de préciser %): `40`
    - Chaque ligne de la table de butin a une chance d'être tirée indépendamment des autres. La somme des probabilités peut donc être supérieure à 100%.
    - Lorsqu'un item looté a un intervalle, le nombre d'items obtenus correspond à un nombre aléatoire est choisi dans cet intervalle.
    - L'intervalle `min-max` peut être remplacé par un chiffre unique si min et max sont identiques: `2-2` devient `2`
    - __Facultatif__ : Si le butin est en quantité limitée, le paramètre stock peut être indiqué: `stock:10`
    - __Facultatif__ : Le mot-clé `secret` peut être utilisé pour que l'item n'apparaisse pas dans les tables de butin dans le wiki.
    - Si un stock tombe à 0, les items sont supprimés de la table et un message est envoyé dans le salon de logs.
    ~~-----------------------------------------------------------------~~
    ## Sets
    ```
    ­[nom du set]:[probabilité en %], [min-max], stock:[nombre] , secret
    - [nom de l'item]:[poids], [min-max], stock:[nombre], secret
    - ...
    ```
    - La déclaration du set est identique à celle d'un item.
    - Le nom des sets est purement technique et ne sera pas affiché dans les wiki.
    - Les sets contiennent une liste d'items.
    - Les items des sets ont la même syntaxe que les items seuls.
    - Les items des sets sont **exclusifs** entre eux.
    - La probabilité des éléments des sets est **relative** au total.
    - Si un intervalle est défini pour le set, les items sont piochés indépendamment dans le set autant de fois que le nombre aléatoire tiré dans l'intervalle.
    - Chaque item d'un set peut posséder son propre stock.
    - Le set peut posséder son propre stock. Si le stock du set tombe à 0, le set est supprimé même si des items du set ont encore du stock.
    - Chaque item du set est distingué des autres items seuls de la table de butin par un '-' au début.
    ~~-----------------------------------------------------------------~~
    Exemple en pratique :
    ```
    or: 40, 5-20
    epee legendaire: 5, 1, stock:1, secret

    armure_chevalier: 20, 1-5, stock:5
    - plastron: 5, 1,  stock:4
    - jambiere: 5, 1-2
    - gantelet: 5, 1-2 stock:6
    - grimoire: 1, 1, secret
    - chevalière: 1, 1, stock:1, secret
    ```
    Notes:
    - L'intervalle du set 1-5 indique que jusqu'à 5 items du set peuvent être piochés de façon indépendante.
    - Le stock du set (5) est inférieur au stock total du set (4+6+1 = 11). Cela implique que le set sera probablement détruit avant l'écoulement des stocks des items.
    - La somme des poids est de 5+5+5+1+1 = 17. Cela signifie que les `plastron`, `jambière` et `gantelet` ont une chance de 5/17 d'être piochés, tandis que le `grimoire` et la `chevalière` ont une chance de 1/17 d'être piochés.
    - Si la chevalière est piochée, son stock arrivera à 0. Donc la somme des poids est alors de 16, ce qui chanque les probabilités: 5/16 pour les `plastron`, `jambière` et `gantelet`, et 1/16 pour le `grimoire`.

loot_table__server_not_found = Serveur non trouvé
    .title = Serveur non trouvé
    .message = Le serveur n'as pas été trouvé.
        Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
loot_table__no_permission = Permissions insuffisantes
    .title = Permissions insuffisantes
    .message = Vous n'avez pas la permission de gérer les tables de butin.
loot_table__target_not_found = Cible inconnue
    .title = Cible inconnue
    .message = Cible invalide : doit être une catégorie de lieu, un salon de route ou un sous-salon de lieu
loot_table__slash_only = Slash commande
    .title = Slash commande
    .message = Cette commande ne peut être utilisée que comme commande slash
loot_table__success = Table de butin enregistrée
    .title = Table de butin enregistrée
    .message = Table de butin enregistrée avec succès
loot_table__invalid_min_max = Plage invalide
    .title = Plage invalide
    .message = Plage de quantité invalide : {$min} à {$max}. Min doit être <= max.
loot_table__invalid_item_name = Nom d'item invalide
    .title = Nom d'item invalide.
    .message = Nom d'objet ou de set invalide : {$name}
loot_table__modified = Loot table modifiée
    .title = Loot table modifiée
    .message = La loot table à été modifiée avec succès.
loot_table__not_in_guild = Pas une guilde
    .title = Pas une guilde
    .message = Cette commande doit être utilisée dans un serveur.
loot_table__universe_not_found = Univers non trouvé
    .title = Univers non trouvé
    .message = L'univers associé à ce serveur n'a pas pu être trouvé.
loot_table__character_not_found = Personnage non trouvé
    .title = Personnage non trouvé
    .message = Vous n'avez pas de personnage dans cet univers. Veuillez en créer un d'abord.
loot_table__error_fetching_universe = Erreur lors de la récupération de l'univers
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de la récupération de l'univers : {$error}
loot_table__error_fetching_character = Erreur lors de la récupération du personnage
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de la récupération de votre personnage : {$error}
loot_table__error_fetching_channel_table = Erreur de table de butin du salon
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de la récupération de la table de butin du salon : {$error}
loot_table__error_fetching_category_table = Erreur de table de butin de la catégorie
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de la récupération de la table de butin de la catégorie : {$error}
loot_table__error_adding_inventory = Erreur d'inventaire
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de l'ajout des objets à votre inventaire : {$error}
loot_table__setup_channel = Salon de configuration
    .title = Salon de configuration
    .message = Vous ne pouvez pas fouiller dans un salon de configuration.
loot_table__no_loot_found = Rien trouvé
    .title = Rien trouvé
    .message = Vous avez fouillé partout, mais vous n'avez rien trouvé cette fois-ci.
loot_table__loot_success = Fouille réussie
    .title = Fouille réussie
    .message = Vous avez trouvé des objets ! {$items}
loot_table__deleted_log = La table de butin du salon <#{$channel_id}> a été supprimée car elle est désormais vide.
loot_table__rate_limited = Temps de recharge
    .title = Temps de recharge
    .message = Vous devez attendre encore {$error} secondes avant de pouvoir fouiller à nouveau cette zone.
loot_table__loot_started = Fouille commencée
    .title = Fouille commencée
    .message = Vous commencez à fouiller la zone. Cela va prendre {$delay} secondes.
loot_table__loot_finished_title = Fouille terminée
loot_table__loot_finished_message = Votre fouille dans l'univers **{$universe}** avec **{$character}** est terminée !
loot_table__loot_finished_late_message = Votre fouille dans l'univers **{$universe}** avec **{$character}** s'est terminée avec succès (un peu tardivement pour des raisons techniques) !
loot_table__empty_loot = Aucun objet obtenu.
loot_table__already_moving = Voyage en cours
    .title = Action impossible
    .message = Vous ne pouvez pas fouiller pendant que vous voyagez.
loot_table__already_crafting = Craft en cours
    .title = Action impossible
    .message = Vous ne pouvez pas fouiller pendant que vous fabriquez quelque chose.
loot_table__already_looting = Fouille déjà en cours
    .title = Action impossible
    .message = Vous êtes déjà en train de fouiller une zone.
loot_table__not_in_loot = Pas de fouille en cours
    .title = Action impossible
    .message = Vous n'êtes pas en train de fouiller actuellement.
loot_table__stopped = Fouille annulée
    .title = Succès
    .message = Votre fouille a été annulée.
create_item__invalid_name = Nom d'objet invalide
    .title = Nom incorrect
    .message = Le nom de l'objet **{$name}** contient des caractères non autorisés. Seuls les caractères alphanumériques, espaces, tirets et underscores sont autorisés.
create_item__already_exists = Un objet avec ce nom existe déjà dans cet univers.
    .title = Création impossible
    .message = Un objet portant ce nom existe déjà dans cet univers.

#Stats
stat_insert__failed = Échec de l'insertion des statistiques
    .title = Ajout de la stat échouée
    .message = La stat n'as pas pu être ajoutée.
resolve_stat__character_not_found = Personnage non trouvé lors de la résolution de la stat
    .title = Erreur de statistique
    .message = Impossible de trouver le personnage pour calculer ses statistiques.
resolve_stat__database_error = Erreur de base de données lors de la résolution de la stat
    .title = Erreur de statistique
    .message = Une erreur de base de données s'est produite lors de la récupération des statistiques.
#Reply
reply__reply_success = Succès
    .title = Succès
    .message = L'opération a été effectuée avec succès.
reply__reply_failed = Échec de l'envoi de la réponse
    .title = Réponse échouée
    .message = La réponse a échouée
#Universe
universe = univers
    .description = Commandes de gestion de l'univers.
universe_create_universe = nouvel_univers
    .description = Permet de créer un nouvel univers. Un serveur ne peut être rattaché qu'à un univers à la fois.
    .universe_name = nom
    .universe_name-description = Nom du nouvel Univers
    .setup_type = type_de_setup
    .setup_type-description = Type de configuration pour ce serveur
universe_add_server = ajouter_serveur
    .description = Ajoute ce serveur à un univers existant.
    .setup_type = type_de_setup
    .setup_type-description = Type de configuration pour ce serveur
universe_setup = configuration
    .description = Configure ou reconfigure le serveur actuel pour l'univers auquel il est lié.
    .setup_type = type_de_setup
    .setup_type-description = Type de configuration à effectuer (Complet ou Partiel).
universe_time = temps
    .description = Affiche l'heure actuelle de l'univers.

#Places
place = lieu
    .description = Commandes de gestion des lieux.
place_create_place = nouvel_endroit
    .description = Crée une nouvelle catégorie correspondant à une ville ou un lieu d'interaction.
    .name = nom
    .name-description = Nom du lieu à créer.

#Roads
road = route
    .description = Commandes de gestion des routes.
road_create_road = nouvelle_route
    .description = Crée une nouvelle route entre deux lieux.
    .place_one = lieu_un
    .place_one-description = Première extrémité de la route.
    .place_two = lieu_deux
    .place_two-description = Seconde extrémité de la route.
    .distance = distance
    .distance-description = Distance entre les deux lieux en kilomètres.
    .secret_channel = secret
    .secret_channel-description = Si vrai, la route ne sera pas affichée sur les cartes publiques.
create_place__new_place_title = Lieu: {$place_name}
create_place__channel_id = Id du lieu

#Characters
character = personnage
    .description = Commandes de gestion des personnages.
character_create = nouveau
    .description = Permet de créer votre personnage dans l'univers. Un seul personnage par joueur.
character_inventory = inventaire
    .description = Permet au joueur de consulter l'inventaire de son personnage.

#Travels
travel = voyage
    .description = Permet de se déplacer d'un lieu à un autre.
travel_start = départ
    .description = Commence un voyage vers une destination.
    .destination = destination
    .destination-description = Le lieu où vous souhaitez vous rendre (ID ou mention).
travel_stop = stop
    .description = Arrêter votre voyage actuel sur la route où vous vous trouvez.
travel_join = rejoindre
    .description = Rejoindre le groupe de voyage d'un autre joueur.
    .target = joueur
    .target-description = Le joueur dont vous souhaitez rejoindre le groupe.
travel_leave = quitter
    .description = Quitter votre groupe de voyage actuel.
travel_estimate = estimer
    .description = Estimer la distance et le temps pour rejoindre un autre joueur sur la route.
    .target = joueur
    .target-description = Le joueur cible.
ping = ping
    .description = Mesure la latence du bot.
support = supporter
    .description = Affiche les informations pour soutenir le projet.
start = start
    .description = Affiche les instructions de démarrage.

#Server
id__nothing_to_delete = Rien à supprimer
    .title = Rien à supprimer
    .message = Aucun élément à supprimer n'a été trouvé
id__role_delete_success = Rôle supprimé avec succès
    .title = Suppression réussie
    .message = Le rôle a été supprimé avec succès
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
id__role_delete_failed = Échec de la suppression du rôle
    .title = Erreur de suppression
    .message = Impossible de supprimer le rôle
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
id__channel_delete_sucess = Salon supprimé avec succès
    .title = Suppression réussie
    .message = Le salon a été supprimé avec succès
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
id__channel_delete_failed = Échec de la suppression du salon
    .title = Erreur de suppression
    .message = Impossible de supprimer le salon
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}

#Setup
SetupType = SetupType
FullSetup = Complet
    .description = Configuration complète avec tous les salons
PartialSetup = Partiel
    .description = Configuration partielle (uniquement routes et rôles)
cancel_setup = Annuler
continue_setup = Continuer 
setup__continue_setup_message = Continuer la configuration ?
    .title = Continuer la configuration
    .message = Voulez-vous continuer la configuration malgré un précédent setup ?  Les salons et rôles inexistants seront créés.
setup__server_already_setup_timeout = Délai de configuration dépassé
    .title = Délai dépassé
    .message = Le délai pour continuer la configuration a expiré
partial_setup__get_guild_roles_error = Échec de la récupération des rôles du serveur
    .title = Erreur de configuration
    .message = Impossible de récupérer les rôles du serveur.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__server_not_found = Serveur introuvable
    .title = Serveur introuvable
    .message = Ce serveur n'est pas enregistré dans notre base de données.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup_server__cancelled = Configuration annulée
    .title = Configuration annulée
    .message = La configuration du serveur a été annulée
setup_server__success = Configuration réussie
    .title = Succès
    .message = Le serveur a été configuré avec succès
setup_server__failed = Échec de la configuration
    .title = Erreur
    .message = La configuration du serveur a échoué
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__full_setup_success = Configuration complète réussie
    .title = Configuration terminée
    .message = La configuration complète du serveur a été effectuée avec succès
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
admin_category_name = Administration
    .title = Administration
    .message = Catégorie d'administration
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__admin_category_not_created = Catégorie d'administration non créée
    .title = Erreur de création
    .message = Impossible de créer la catégorie d'administration
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
nrp_category_name = Hors RP
setup__nrp_category_not_created = Catégorie Hors RP non créée
    .title = Erreur de création
    .message = Impossible de créer la catégorie Hors RP
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
rp_category_name = RP
setup__rp_category_not_created = Catégorie RP non créée
    .title = Erreur de création
    .message = Impossible de créer la catégorie RP
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__roles_setup_failed = Échec de la configuration des rôles
    .title = Erreur de configuration
    .message = La configuration des rôles a échoué
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
log_channel_name = Logs
setup__log_channel_not_created = Salon de logs non créé
    .title = Erreur de création
    .message = Impossible de créer le salon de log
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
commands_channel_name = Commandes
setup__commands_channel_not_created = Salon de commandes non créé
    .title = Erreur de création
    .message = Impossible de créer le salon de commandes
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
moderation_channel_name = Moderation
setup__moderation_channel_not_created = Salon de modération non créé
    .title = Erreur de création
    .message = Impossible de créer le salon de modération
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
nrp_general_channel_name = General
setup__nrp_general_channel_not_created = Salon général Hors RP non créé
    .title = Erreur de création
    .message = Impossible de créer le salon général Hors RP
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
rp_character_channel_name = Fiches personnages
setup__rp_character_channel_not_created = Salon de fiches personnages non créé
    .title = Erreur de création
    .message = Impossible de créer le salon de fiches personnages
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
universal_time_channel_name = Temps universel
setup__universal_time_channel_not_created = Salon de temps universel non créé
    .title = Erreur de création
    .message = Impossible de créer le salon de temps universel
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
rp_wiki_channel_name = Wiki
setup__wiki_channel_not_created = Salon wiki non créé
    .title = Erreur de création
    .message = Impossible de créer le salon wiki
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__rollback_failed = Échec de l'annulation des modifications
    .title = Erreur d'annulation
    .message = Impossible d'annuler les modifications effectuées
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__channel_setup_failed = Échec de la configuration des salons
    .title = Erreur de configuration
    .message = La configuration des salons a échoué
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
guild_only = Commande réservée aux serveurs.
admin_role_name = Administrateur
setup__admin_role_not_created = Rôle Administrateur non créé
    .title = Erreur de création
    .message = Impossible de créer le rôle Administrateur
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
moderator_role_name = Modérateur
setup__moderator_role_not_created = Rôle Modérateur non créé
    .title = Erreur de création
    .message = Impossible de créer le rôle Modérateur
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
spectator_role_name = Spectateur
setup__spectator_role_not_created = Rôle Spectateur non créé
    .title = Erreur de création
    .message = Impossible de créer le rôle Spectateur
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
player_role_name = Joueur
setup__player_role_not_created = Rôle Joueur non créé
    .title = Erreur de création
    .message = Impossible de créer le rôle Joueur
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__error_during_role_creation = Erreur lors de la création des rôles
    .title = Erreur de création
    .message = Une erreur s'est produite lors de la création des rôles
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__reorder_went_wrong = Erreur lors du réordonnancement
    .title = Erreur
    .message = Le réordonnancement des salons ou rôles a échoué.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}

create_universe__check_universe_limit_failed = Échec de la vérification de la limite
    .title = Erreur
    .message = Impossible de vérifier votre limite d'univers.

create_universe__universe_limit_reached = Limite d'univers atteinte
    .title = Limite atteinte
    .message = Vous avez atteint le nombre maximum d'univers autorisés pour votre compte.

create_universe__get_server_failed = Échec de récupération du serveur
    .title = Erreur
    .message = Impossible de récupérer les informations du serveur Discord.

create_universe__already_exist_for_this_server = Univers déjà existant
    .title = Erreur
    .message = Un univers est déjà associé à ce serveur Discord.

create_universe__universe_insert_failed = Échec de création de l'univers
    .title = Erreur
    .message = L'enregistrement de l'univers en base de données a échoué.

create_universe__setup_constraints_failed = Échec de configuration des contraintes
    .title = Erreur
    .message = La configuration des contraintes d'unicité pour l'univers a échoué.

create_universe__server_insert_failed = Échec d'enregistrement du serveur
    .title = Erreur
    .message = L'association du serveur à l'univers a échoué.

create_universe__speed_stat_insert_failed = Échec d'initialisation des statistiques
    .title = Erreur
    .message = L'initialisation de la statistique de vitesse a échoué.

create_universe__universe_successfully_created = Univers créé avec succès
    .title = Succès
    .message = Votre univers a été créé et configuré avec succès !
road_channel_name = Routes
setup__road_category_not_created = Catégorie Routes non créée
    .title = Erreur de création
    .message = Impossible de créer la catégorie Routes
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__server_update_failed = Échec de la mise à jour du serveur
    .title = Erreur de mise à jour
    .message = Impossible de mettre à jour les informations du serveur
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
setup__setup_success_message = Configuration terminée avec succès
    .title = Succès
    .message = La configuration a été effectuée avec succès

#create place
create_placce = nouvel_endroit
create_place__server_not_found = Serveur inconnu
    .title = Server inconnu
    .message = Le serveur semble ne pas être enregistré. Faites /{$universe} {$add_server} [type de setup]
create_place__database_not_found = Base de données introuvable
    .title = Connexion échouée
    .message = La connexion à la base de donénes à échouée.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_place__role_not_created = Création de rôle échouée
    .title = Création de rôle échouée
    .message = Le rôle du lieu n'as pas pu être créé correctement.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_place__rollback_complete = Rollback terminé
    .title = Rollback effectué
    .message = Quelque chose s'est mal passé durant la création du lieu. Un rollback à été effectué.
create_role__rollback_failed = Rollback échoué
    .title = Rollback échoué
    .message = Quelque chose s'est mal passé durant la création du lieu et le rollback à échoué.
            Veuillez contacter le support : {support_link}
create_place__success = Place créée
    .title = Place créée
    .message = La place à été créée avec succès.

#Create road
create_road = nouvelle_route
create_road__server_not_found = Serveur introuvable
    .title = Serveur introuvable
    .message = Le serveur ne semble pas être enregistré. Faites /{$universe} {$add_server} [type de setup]
create_road__database_error = Erreur de base de données
    .title = Erreur de base de données
    .message = Une erreur s'est produite lors de l'accès à la base de données.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_place__place_one_not_found = Premier lieu introuvable
    .title = Premier lieu introuvable
    .message = Le premier lieu spécifié n'a pas été trouvé dans l'univers.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_place__place_two_not_found = Second lieu introuvable
    .title = Second lieu introuvable
    .message = Le second lieu spécifié n'a pas été trouvé dans l'univers.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_road__role_creation_failed = Erreur de création de rôle
    .title = Erreur de création de rôle
    .message = Le rôle de la route n'a pas pu être créé correctement.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_road__create_channel_failed_rollback_success = Erreur de création de salon
    .title = Erreur de création de salon
    .message = Le salon n'a pas pu être créé, mais les modifications ont été annulées.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_road__create_channel_failed_rollback_failed = Erreur critique
    .title = Erreur critique
    .message = La création du salon a échoué et le rollback n'a pas pu être effectué.
            Veuillez contacter le support : {support_link}
create_road__insert_road_failed_rollback_success = Erreur d'insertion
    .title = Erreur d'insertion
    .message = La route n'a pas pu être sauvegardée, mais les modifications ont été annulées.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_road__insert_road_failed_rollback_channel_failed = Erreur critique
    .title = Erreur critique
    .message = L'enregistrement de la route a échoué et l'annulation du salon a échoué.
            Veuillez contacter le support : {support_link}
create_road__insert_road_failed_rollback_role_failed = Erreur critique
    .title = Erreur critique
    .message = L'enregistrement de la route a échoué et l'annulation du rôle a échoué.
            Veuillez contacter le support : {support_link}
create_road__invalid_place_one = Identifiant du premier lieu invalide
    .title = Premier lieu invalide
    .message = L'identifiant ou la mention du premier lieu est invalide. Utilisez un ID ou une mention <#id>.
create_road__invalid_place_two = Identifiant du second lieu invalide
    .title = Second lieu invalide
    .message = L'identifiant ou la mention du second lieu est invalide. Utilisez un ID ou une mention <#id>.
create_road__success = Route créée
    .title = Route créée
    .message = La route a été créée avec succès
create_road__limit_reached = Limite de routes atteinte
    .title = Limite atteinte
    .message = L'un des lieux a déjà atteint le nombre maximum de 25 routes (hors routes secrètes).
create_road__already_exists = Route déjà existante
    .title = Route existante
    .message = Une route existe déjà entre ces deux lieux.
create_road__universe_mismatch = Univers différent
    .title = Univers différent
    .message = Les deux lieux doivent appartenir au même univers.

#Create character
create_character = nouveau_personnage
character_modal_title = Créer un nouveau personnage
create_character__delete_character = Annuler
create_character__submit_character = Envoyer
create_character__modify_character = Modifier
create_character__refuse_character = Refuser
create_character__accept_character = Accepter
character_special_request = Requêtes spéciales
character_story = Histoire du personnage
character_description = Description physique
character_name = Nom du personnage
create_character__start_place = Lieu de départ
create_character__submit_notification = @here Une fiche de personnage est en attente de vérification :

character_reject_reason = Raison du refus

create_character__no_universe_found = Univers introuvable
    .title = Univers introuvable
    .message = Il n'y a pas d'univers existant pour ce serveur.
create_character__database_error = Erreur de base de données
    .title = Erreur de base de données
    .message = Impossible d'accéder à la base de données.
            Veuillez réessayer ou contacter le support si le problème persiste : {support_link}
create_character__wrong_channel = Mauvais salon
    .title = Mauvais salon
    .message = Cette commande doit être utilisée dans le salon des fiches de personnage.
create_character__character_already_existing = Le personnage existe déjà
    .title = Le personnage existe déjà
    .message = Vous avez déjà un personnage. Vous ne pouvez pas en créer un autre.
CharacterModal = character_modal
    .character_name = Nom
    .character_description = Description du personnage
    .placeholder = Décrivez votre personnage ici...
    .character_story = Histoire du personnage
    .value = Il était une fois...
    .character_special_request = Requêtes spéciales
create_character__modal_opened = Formulaire ouvert
    .title = Création de personnage
    .message = Le formulaire de création de personnage a été ouvert.
create_character__submitted = Personnage envoyé
    .title = Personnage envoyé
    .message = Votre fiche de personnage a été envoyée pour vérification. Veuillez attendre la décision d'un modérateur.
create_place__character_too_long = Fiche de personnage trop longue
    .title = Fiche de personnage trop longue
    .message = La fiche de personnage est trop longue pour être affichée. Veuillez réessayer.
character_instruction = Remplissez les champs suivants pour décrire votre personnage.
    ► Tous les champs de paragraphe sont limités à 1024 caractères.
    ► Un délai de 30 minutes est configuré par sécurité.
    Vous pouvez cliquer sur le bouton modifier pour changer votre brouillon avant de l'envoyer aux modérateurs.
create_character__timed_out = Délai dépassé
    .title = Délai dépassé
    .message = Le processus de création de personnage a expiré.
create_character__guild_only = Serveur uniquement
    .title = Serveur uniquement
    .message = Cette commande ne peut être utilisée qu'au sein d'un serveur.
create_character__delete_successfull = Annulé
    .title = Création de personnage annulée
    .message = Votre processus de création de personnage a été annulé avec succès.
delete_character = Personnage supprimé
    .title = Personnage supprimé
    .message = La fiche de personnage a été supprimée avec succès.
create_character__not_owner = Pas le propriétaire
    .title = Pas le propriétaire
    .message = Vous n'êtes pas le propriétaire de ce personnage. Vous ne pouvez pas effectuer cette action.
create_character__no_member = Membre introuvable
    .title = Erreur
    .message = Impossible de trouver les informations du membre.
create_character__no_permission = Permission refusée
    .title = Permission refusée
    .message = Vous n'avez pas les permissions requises (Modérateur ou Administrateur) pour effectuer cette action.
create_character__invalid_footer = Interaction invalide
    .title = Erreur
    .message = Les métadonnées de l'interaction sont invalides.
create_character__invalid_embed_title = Titre d'embed invalide
    .title = Erreur
    .message = Le titre de la fiche de personnage est invalide.
create_character__message_not_found = Message introuvable
    .title = Erreur
    .message = Le message de la fiche de personnage n'a pas pu être trouvé.
create_character__refused = Personnage refusé
    .title = Personnage refusé
    .message = Le personnage a été refusé par un modérateur.
accept_character = Personnage accepté
    .title = Personnage accepté
    .message = Le personnage a été accepté avec succès et ajouté à l'univers.
create_character__type_mismatch = Incompatibilité de type
    .title = Erreur de validation
    .message = L'une des valeurs de statistiques fournies ne correspond pas au type attendu.
create_character__invalid_place_selected = Lieu invalide sélectionné. Veuillez choisir une catégorie valide pour le personnage.
create_character__invalid_interaction = Données d'interaction invalides.
create_character__choose_place = Choisir un lieu
    .title = Choisir un lieu
    .message = Veuillez sélectionner la catégorie où le personnage sera situé.
character_stat_input = Statistiques du personnage
accept_character__nickname_error = Erreur de changement de pseudo
    .title = Erreur de pseudo
    .message = Le bot n'a pas pu modifier votre pseudo. Cela arrive souvent pour les administrateurs. Un modérateur devra le faire manuellement.
accept_character__nickname_skipped_log = ⚠️ Le pseudo de {$user} ({$character}) n'a pas pu être modifié automatiquement (probablement un administrateur).
accept_character__no_player_role_id = Serveur non setup
    .title = Serveur non setup
    .message = Le role {player_role_name} n'as pas été trouvé.
accept_character__member_not_found = Membre introuvable lors de l'acceptation.
    .title = Erreur d'acceptation
    .message = Impossible de trouver l'utilisateur sur le serveur.


#Travels
travel__server_not_found = Serveur introuvable
    .title = Serveur introuvable
    .message = Le serveur ne semble pas être enregistré.
travel__place_not_found = Lieu introuvable
    .title = Lieu introuvable
    .message = Le lieu de destination spécifié n'existe pas dans cet univers.
travel__road_not_found = Route introuvable
    .title = Route introuvable
    .message = Vous ne vous trouvez pas sur une route valide.
travel__character_not_found = Personnage introuvable
    .title = Personnage introuvable
    .message = Vous n'avez pas de personnage dans cet univers.
travel__database_error = Erreur de base de données
    .title = Erreur de base de données
    .message = Une erreur est survenue lors de l'accès à la base de données.
travel_without_destination__database_error = Erreur de base de données
    .title = Erreur de base de données
    .message = Impossible de récupérer les routes disponibles.
travel_without_destination__reply_failed = Erreur d'envoi
    .title = Erreur d'envoi
    .message = Impossible d'afficher le menu de sélection de destination.
travel__source_place_not_found = Lieu d'origine introuvable
    .title = Lieu d'origine introuvable
    .message = Votre position actuelle n'est pas reconnue comme un lieu valide.
travel__started = Voyage commencé
    .title = Voyage commencé
    .message = Votre groupe s'est mis en route.
travel__stopped = Voyage arrêté
    .title = Voyage arrêté
    .message = Votre voyage a été arrêté. Vous pouvez maintenant choisir une destination ou rester ici.
travel__interrupted = Voyage interrompu
    .title = Voyage interrompu
    .message = Vous avez arrêté votre voyage.
travel__not_in_move = Pas de voyage en cours
    .title = Pas de voyage en cours
    .message = Vous n'êtes pas en train de voyager.
travel__already_moving_to_destination = Déjà en route
    .title = Déjà en route
    .message = Vous êtes déjà en train de vous déplacer vers cette destination.
travel__invalid_road_destination = Destination invalide
    .title = Destination invalide
    .message = Vous ne pouvez pas aller à cet endroit depuis votre position actuelle sur la route.
move_from_place__road_not_found = Aucune route trouvée
    .title = Aucune route trouvée
    .message = Il n'y a pas de route directe entre votre position actuelle et {$destination}.

travel__rejoining_route = _**{$user}** se dirige vers **{$destination}**._
travel__moving_to_place = _**{$user}** se déplace vers **{$destination}**._
travel__reached_destination = _**{$user}** arrive à **{$destination}**._
travel__arrived_at_destination = _**{$user}** vient d'arriver._
travel__interrupted_towards = _**{$user}** interrompt son trajet._
travel__moving_to_place_from_start = _**{$user}** prend la route vers **{$destination}**._
travel__taking_unknown_road = _**{$user}** emprunte une route inconnue._
travel__invitation = Frontière atteinte
    .title = Frontière atteinte
    .message = **_{$user}, tu arrive à la frontière d'une région de l'univers {$universe} ! Voici l'invitation pour continuer ton voyage : {$link} _**
travel__no_road_available = Aucune route disponible
    .title = Aucune route disponible
    .message = Aucune route ne semble disponible depuis cet endroit. Peut-être qu'une route secrète existe ?

#Recipes
recipe = recette
    .description = Gérer les recettes de fabrication
recipe_create = créer
    .description = Créer une nouvelle recette
    .name = nom
    .name-description = Nom de la recette
    .delay = délai
    .delay-description = Délai de fabrication en secondes
recipe_craft = fabriquer
    .description = Fabriquer un objet à partir d'une recette
    .recipe_name = nom_recette
    .recipe_name-description = Nom de la recette à utiliser
recipe_stop = arrêter_fabrication
    .description = Arrêter la fabrication en cours

recipe__modal_title = Éditeur de recette
recipe__modal_field_name = Contenu de la recette
recipe__modal_placeholder = # Guide de syntaxe des Recettes
    - `> [nom de l'item] [quantité]` : Item obtenu (résultat).
    - `< [nom de l'item] [quantité]` : Item utilisé (ingrédient).
    - `- [nom de l'item]` : Outil nécessaire (non consommé).
    
    Exemple :
    ```
    - Enclume
    - Marteau
    < Lingot de fer 2
    < Charbon 1
    > Épée en fer 1
    ```

recipe__server_not_found = Serveur non trouvé
    .title = Serveur non trouvé
    .message = Le serveur n'a pas été trouvé.
recipe__character_not_found = Personnage non trouvé
    .title = Personnage non trouvé
    .message = Vous n'avez pas de personnage dans cet univers.
recipe__item_not_found = Objet non trouvé
    .title = Objet non trouvé
    .message = L'objet "{$name}" n'existe pas dans la base de données de cet univers.
recipe__not_found = Recette non trouvée
    .title = Recette non trouvée
    .message = Aucune recette nommée "{$recipe_name}" n'a été trouvée.
recipe__missing_tool = Outil manquant
    .title = Outil manquant
    .message = Il vous manque un ou plusieurs outils nécessaires pour cette recette.
recipe__missing_ingredient = Ingrédient manquant
    .title = Ingrédient manquant
    .message = Vous n'avez pas tous les ingrédients nécessaires dans votre inventaire.
recipe__create_success = Recette créée
    .title = Succès
    .message = La recette a été enregistrée avec succès.
recipe__no_permission = Vous n'avez pas la permission de créer une recette.
    .title = Erreur
    .message = Vous devez être administrateur ou avoir le rôle de joueur pour créer une recette.
recipe__submit_success = Soumission réussie
    .title = Succès
    .message = Votre recette a été soumise pour approbation.
recipe__validation_title = Validation de Recette
recipe__delay_field = Délai (sec)
recipe__creator_field = Créateur
recipe__into_wiki_field = Wiki
recipe__submit_notification = @here Une nouvelle fiche de recette est en attente de vérification :
recipe__approve = Approuver
recipe__reject = Refuser
recipe__modify = Modifier
recipe__approved = Recette approuvée
    .title = Approuvé
    .message = La recette a été approuvée et enregistrée.
recipe__rejected = Recette refusée
    .title = Refusé
    .message = La recette a été refusée.
recipe__modified = Recette modifiée
    .title = Modifié
    .message = La recette a été modifiée avec succès.
recipe__invalid_embed = Embed invalide
    .title = Erreur
    .message = L'embed de la recette est invalide ou corrompu.
recipe__no_embed = Embed manquant
    .title = Erreur
    .message = Impossible de trouver l'embed de la recette.
recipe__no_content = Contenu manquant
    .title = Erreur
    .message = La recette ne contient aucun texte.
recipe__guild_only = Serveur requis
    .title = Erreur
    .message = Cette action doit être effectuée dans un serveur Discord.
recipe__craft_success = Fabrication réussie
    .title = Fabrication réussie: {$recipe_name}
    .message = Vous avez fabriqué l'objet avec succès !
recipe__craft_started = Fabrication commencée
    .title = Fabrication de {$recipe_name}
    .message = La fabrication a commencée. Temps restant : **{$delay}** secondes.
recipe__craft_finished_title = Craft terminé
recipe__craft_finished_message = Vous avez terminé la fabrication de **{$recipe_name}** dans l'univers **{$universe}**.
recipe__craft_finished_late_message = Votre fabrication de **{$recipe_name}** dans l'univers **{$universe}** est terminée ! Elle a pris du retard pour des raisons techniques, nous vous prions de nous en excuser.
recipe__craft_stopped = Craft annulé
    .title = Annulation réussie
    .message = Le craft en cours a été annulé. (Les ingrédients déjà consommés ne sont pas rendus)
recipe__no_craft_in_progress = Aucun craft en cours
    .title = Erreur
    .message = Vous n'avez aucun craft en cours.
recipe__cannot_craft_while_moving = Déplacement en cours
    .title = Erreur
    .message = Vous ne pouvez pas crafter pendant que vous vous déplacez.
recipe__craft_already_in_progress = Craft déjà en cours
    .title = Erreur
    .message = Vous avez déjà un craft en cours.
travel__cannot_move_while_crafting = Craft en cours
    .title = Erreur
    .message = Vous ne pouvez pas vous déplacer pendant que vous craftez. Utilisez `/recipe stop` pour annuler le craft.

travel__cannot_join_self = Action impossible
    .title = Action impossible
    .message = Vous ne pouvez pas rejoindre votre propre groupe.

travel__universe_not_found = Univers introuvable
    .title = Univers introuvable
    .message = L'univers associé à ce serveur n'a pas été trouvé.

travel__target_not_found = Joueur introuvable
    .title = Joueur introuvable
    .message = Le joueur cible n'a pas été trouvé ou ne possède pas de personnage.

travel__already_in_same_group = Déjà dans le groupe
    .title = Déjà dans le groupe
    .message = Vous faites déjà partie du groupe de ce joueur.

travel__too_far_different_place = Trop loin
    .title = Trop loin
    .message = Vous n'êtes pas au même endroit que ce joueur.

travel__too_far_to_join = Trop loin
    .title = Trop loin
    .message = Vous êtes trop loin de ce joueur pour rejoindre son groupe en mouvement.

travel__joined_group = Groupe rejoint
    .title = Groupe rejoint
    .message = Vous avez rejoint le groupe.

travel__wrong_channel =
    .title = Mauvais salon
    .message = Vous n'êtes pas au bon endroit pour faire cela. Votre personnage se trouve actuellement dans la catégorie **{$category}**, salon **{$channel}**.

travel__already_alone = Déjà seul
    .title = Déjà seul
    .message = Vous êtes déjà seul dans votre groupe.

travel__left_group = Groupe quitté
    .title = Groupe quitté
    .message = Vous avez quitté le groupe.

travel__only_leader_can_stop = Chef de groupe uniquement
    .title = Chef de groupe uniquement
    .message = Seul le chef du groupe peut arrêter le voyage. Vous pouvez quitter le groupe avec la commande `/voyage quitter`.

travel__public_joined = `{$user} a rejoint le groupe de {$target}.`
travel__public_left = `{$user} a quitté le groupe de {$leader}.`

travel__cannot_estimate_self = Action impossible
    .title = Action impossible
    .message = Vous ne pouvez pas estimer la distance vers vous-même.

travel__not_on_same_road = Pas sur la même route
    .title = Pas sur la même route
    .message = Vous et le joueur cible devez être sur la même route pour faire une estimation.

travel__speed_stat_not_found = Statistique manquante
    .title = Erreur
    .message = La statistique de vitesse n'a pas été trouvée dans cet univers.

travel__speed_resolve_failed = Erreur de calcul
    .title = Erreur
    .message = Impossible de calculer votre vitesse actuelle.

travel__no_speed = Vitesse nulle
    .title = Action impossible
    .message = Votre vitesse est nulle, vous ne pouvez pas estimer de temps de trajet.

travel__estimate_result = Estimation de distance
    .title = Estimation
    .message = Vous estimez que {$target} se trouve à environ **{$distance} mètres** de vous. En jeu, il vous faudrait environ **{$minutes} min {$seconds} s** pour le rejoindre.
travel__estimate_can_join = Proche
    .title = Proche
    .message = {$target} est à portée de ralliement (environ **{$distance} mètres**). En jeu, il vous faudrait environ **{$minutes} min {$seconds} s** pour le rejoindre exactement.

travel__estimate_too_far = Trop loin
    .title = Trop loin
    .message = Vous êtes trop loin de {$target} pour pouvoir estimer sa distance avec précision.
recipe__empty_recipe = Recette vide
    .title = Erreur
    .message = La recette doit contenir au moins un ingrédient ou un résultat.
recipe__invalid_line = Ligne invalide
    .title = Erreur de syntaxe
    .message = Une ligne de la recette est mal formée.
recipe__error_during_consumption = Erreur de consommation
    .title = Erreur critique
    .message = Une erreur est survenue lors de la consommation des ingrédients.
recipe__slash_only = Slash commande uniquement
    .title = Erreur
    .message = Cette commande ne peut être utilisée que comme commande slash.
recipe__recipe_instructions = # Guide de syntaxe des Recettes
    - `> [nom de l'item] [quantité]` : Item obtenu (résultat).
    - `< [nom de l'item] [quantité]` : Item utilisé (ingrédient).
    - `- [nom de l'item]` : Outil nécessaire (non consommé).
    - La quantité est de 1 par défaut si non renseignée.
    - Les lignes commençant par `#` sont des commentaires.
    - __Note__ : Vous devez posséder les outils dans votre inventaire ou ils doivent être placés dans le salon.
    

#Temps Universel
time = temps
universe_time__current_time = Heure de l'univers
    .title = Temps Universel
    .message = Il est actuellement **{$time}** dans cet univers.
        Phase actuelle : **{$phase}**
universe_time__invalid_modifier = Modificateur de temps invalide
    .title = Erreur de temps
    .message = Le modificateur de temps de l'univers est invalide.
time__midnight = **_Il est minuit. Le silence s'abat sur l'univers._**
time__sunrise = **_Le soleil se lève, une nouvelle journée commence._**
time__noon = **_Il est midi. Le soleil est au zénith._**
time__sunset = **_Le soleil se couche, les ombres s'allongent._**

#Items
item = objet
    .description = Groupe de commandes concernant les objets.
item_create = créer
    .description = Permet de créer un nouvel objet
    .name = nom
    .name-description = Nom de l'objet. Il est unique et servira d'identifiant pour les butins.
    .usage = usage
    .usage-description = Type d'usage de l'objet.
    .into_wiki = wiki
    .into_wiki-description = Indique s'il faut ajouter l'objet au wiki.
    .inventory_size = taille_inventaire
    .inventory_size-description = Taille de l'inventaire de l'objet (0 pour aucun)
    .image = illustration
    .image-description = Illustration qui sera affiché pour donner un visuel à l'item.
    .item_description = description
    .item_description-description = Description de l'item.
    .secret_informations = informations_secrètes
    .secret_informations-description = Permet de donner des informations secrètes en plus aux joueurs. Il ne sera pas affiché dans le wiki.
item_lookup = regarder
    .description = Affiche les détails d'un item possédé via son ID d'inventaire
    .id = id
    .id-description = L'ID de la ligne d'inventaire (facultatif si utilisation du carrousel)
item_lookup__title = Regarder un objet
item_lookup__select_placeholder = Sélectionnez un objet à regarder
item_lookup__empty_inventory = Vous n'avez aucun objet à regarder dans votre inventaire.
item_place_command = placer
    .description = Placer un objet dans le salon actuel.
    .inventory_id = id_inventaire
    .inventory_id-description = ID de l'entrée d'inventaire de l'objet à placer
    .immutable = immuable
    .immutable-description = L'objet est-il immuable ? (Admin uniquement)
item_use = utiliser
    .description = Utiliser un objet ou interagir avec un outil/coffre dans le salon actuel.
    .tool_id = id_outil
    .tool_id-description = ID de l'objet avec lequel interagir (facultatif si un seul)
item_usage_title = Type d'usage
item_inventory_size = Taille inventaire
item_lookup_usage = Usage
item_lookup_secret = Informations Secrètes
item_lookup_effects = Effets
item_effects_title = Effets
item_lookup_stat = Stat
item_lookup_value = Valeur
item_lookup_type = Type
item_no_description = _Pas de description_
item_placed_success = Objet placé !
    .title = Objet placé !
    .message = Vous avez placé **{$item_name}** dans **#{$channel_name}**.
item_placed_rp = _**{$item_name}** a été placé par **{$character_name}**._
item_immutable_footer = Cet objet est immuable.

item_effect__modal_title = Effets de l'objet
item_effect__modal_field_name = Liste des effets
item_effect__modal_placeholder = # Guide de syntaxe des Effets
    Stat: Valeur[Type] Durée Niveau
    
    Exemples:
    Force: +5 10m joueur
    Vitesse: x1.2 1h endroit
    HP: 10 flat univers
    
    Types:
    + : Addition
    x : Multiplicateur
    flat : Valeur fixe
    
    Durées: s, m, h, d/j (par défaut: m)
    
    Niveaux:
    joueur : Par défaut
    endroit : Salon ou route
    lieu : Catégorie ou route
    univers : Tout l'univers

ItemUsage = Type d'usage
Consumable = Consommable
    .description = Usage unique, l'objet est détruit après utilisation
Usable = Utilisable
    .description = Usage multiple
Wearable = Équipable
    .description = Peut être porté par le personnage
Placeable = Plaçable
    .description = Peut être placé dans un lieu (ex: coffre, enclume)
None = Autre
    .description = Pas d'usage particulier
inventory__empty = Inventaire vide
    .title = Inventaire vide
    .message = Vous n'avez aucun objet dans votre inventaire.
inventory__empty_description = _L'inventaire est vide._
inventory__refresh_button = Actualiser
inventory__lookup_hint = Utilisez `/item regarder [id]` pour plus de précision.
inventory__title = Inventaire de {$character_name}
inventory__universe_field = Univers
inventory__previous_button = Précédent
inventory__next_button = Suivant
inventory__page_footer = Page {$current} sur {$total} - Utilisez `/item regarder [ID]` pour voir les détails d'un item
inventory__not_in_guild = Serveur uniquement
    .title = Serveur uniquement
    .message = Cette commande doit être utilisée dans un serveur.
item__not_found = Objet non trouvé
    .title = Objet non trouvé
    .message = Aucun objet portant ce nom n'a été trouvé dans cet univers.
item__not_found_in_inventory = Objet non trouvé dans l'inventaire
    .title = Objet non trouvé dans l'inventaire
    .message = Cet ID ne correspond à aucun objet que vous possédez actuellement.
item__not_your_item = Pas votre objet
    .title = Pas votre objet
    .message = Cet objet ne vous appartient pas.
item__invalid_id = ID invalide
    .title = ID invalide
    .message = L'ID d'inventaire fourni est invalide.
item__no_search_criteria = Critères manquants
    .title = Critères manquants
    .message = Veuillez fournir un nom ou un ID pour la recherche.
item__server_not_found = Serveur non trouvé
    .title = Serveur non trouvé
    .message = Le serveur n'a pas été trouvé.
item__not_placeable = Objet non plaçable
    .title = Objet non plaçable
    .message = Cet objet ne peut pas être placé.
item__no_permission = Permissions insuffisantes
    .title = Permissions insuffisantes
    .message = Vous n'avez pas les permissions nécessaires (administrateur ou rôle joueur) pour créer un objet.
item__no_usage = Usage de l'objet non trouvé
    .title = Données manquantes
    .message = L'usage de l'objet n'a pas pu être déterminé à partir du message de validation.
item__no_embed = Embed non trouvé
    .title = Données manquantes
    .message = Impossible de trouver l'embed de validation.
item__guild_only = Serveur uniquement
    .title = Serveur uniquement
    .message = Cette action ne peut être effectuée que sur un serveur.
item__invalid_embed = Embed invalide
    .title = Erreur de données
    .message = La structure de l'embed de validation est incorrecte.
item__not_in_guild_channel = Pas un salon de serveur
    .title = Erreur de salon
    .message = Cette commande doit être utilisée dans un salon de serveur.
item__not_in_category = Pas de catégorie
    .title = Erreur de salon
    .message = Ce salon ne se trouve pas dans une catégorie.
item__not_a_place = Pas un lieu
    .title = Lieu non reconnu
    .message = Ce salon n'est pas associé à un lieu (Place) valide.
item__failed_to_remove = Échec du retrait
    .title = Erreur d'inventaire
    .message = Impossible de retirer l'objet de votre inventaire.
create_item__db_error = Erreur de base de données
    .title = Erreur de création
    .message = Une erreur s'est produite lors de la création de l'objet en base de données.
create_item__validation_title = Validation d'un nouvel objet
create_item__creator_field = Créateur
create_item__into_wiki_field = Inclure au wiki
create_item__secret_field = Informations secrètes
create_item__submit_success = Objet soumis
    .title = Soumission réussie
    .message = Votre objet a été envoyé aux administrateurs pour validation.
create_item__approved = Objet approuvé
    .title = Approbation réussie
    .message = L'objet a été créé et ajouté à la base de données (et au wiki si demandé).
create_item__rejected = Objet rejeté
    .title = Rejet effectué
    .message = La création de l'objet a été refusée.

item__approve = Approuver
item__reject = Rejeter
item_delete = supprimer
    .description = Supprime un objet de l'univers (Admin uniquement).
    .name = nom
    .name-description = Nom de l'objet à supprimer.

item_delete__not_in_guild = Serveur uniquement
    .title = Serveur uniquement
    .message = Cette commande doit être utilisée dans un serveur.
item_delete__server_not_found = Serveur non trouvé
    .title = Serveur non trouvé
    .message = Le serveur n'a pas été trouvé.
item_delete__not_found = Objet non trouvé
    .title = Objet non trouvé
    .message = Aucun objet portant le nom **{$name}** n'a été trouvé dans cet univers.
item_delete__notification_title = Objet Supprimé
item_delete__notification = L'objet **{$item_name}** a été supprimé de l'univers **{$universe_name}** par un administrateur. Il a été retiré de l'inventaire de **{$character_name}**.
item_delete__success = Objet supprimé
    .title = Suppression réussie
    .message = L'objet **{$name}** a été supprimé avec succès de l'univers, des inventaires et du wiki.
    {$affected_recipes_text}
item_delete__affected_recipes = Voici les recettes affectées par la suppression:{$affected_recipes}
item_delete__log_affected_recipes = Recettes impactées (conservées mais incomplètes) par la suppression de **{$item_name}** :
    {$recipes}

use__universe_not_found = Univers non trouvé
    .title = Univers non trouvé
    .message = L'univers associé à ce serveur n'a pas pu être localisé.
use__character_not_found = Personnage non trouvé
    .title = Personnage non trouvé
    .message = Vous devez avoir un personnage créé pour utiliser des objets.
use__invalid_tool_id = ID d'objet invalide
    .title = ID invalide
    .message = L'ID de l'objet fourni n'est pas un identifiant MongoDB valide.
use__no_tools_found = Aucun objet trouvé
    .title = Aucun objet
    .message = Aucun objet utilisable n'a été trouvé dans ce salon.
use__list_tools = Objets disponibles
    .title = Objets disponibles
    .select_placeholder = Sélectionnez un objet à utiliser
    .footer = Page {$current} sur {$total}
use__tool_not_found = Objet non trouvé
    .title = Objet non trouvé
    .message = L'objet spécifié est introuvable ou n'existe plus.
use__no_inventory = Pas d'inventaire
    .title = Usage impossible
    .message = Cet objet ne possède pas d'espace de stockage.
use__only_slash_command = Erreur de commande
    .title = Erreur
    .message = Cette interaction ne peut être initiée que via une commande slash.
use__empty_inventory = L'inventaire est vide.
use__modal_character_inventory_label = Votre inventaire
use__modal_label = Actions de transfert
use__modal_chest_inventory_label = Contenu du coffre
use__modal_instructions_label = Guide de syntaxe
use__modal_instructions_value = # Guide de syntaxe des transactions
    - `> [item_name] [quantité]` : Prendre un ITEM de l'outil.
    - `< [item_name] [quantité]` : Déposer un ITEM dans l'outil.
    - La quantité est de 1 par défaut si non renseignée.
    - __Note__ : Le bilan des items ne doit pas excéder la capacité du coffre.
use__transfer_success = Transfert réussi
    .title = Transfert terminé
    .message = Les objets ont été transférés avec succès.

# Consommation d'items
consume__title = Consommer un objet
consume__empty_inventory = Vous n'avez aucun objet consommable dans votre inventaire.
consume__select_placeholder = Sélectionnez un objet à consommer
consume__success = Vous avez consommé **{ $item_name }** ! Les effets ont été appliqués.
consume__error = Erreur lors de la consommation : { $error }
consume__universe_not_found = Univers non trouvé.
consume__character_not_found = Personnage non trouvé.
consume__busy = Action impossible
    .title = Personnage occupé
    .message = Vous ne pouvez pas consommer d'objet car vous êtes déjà engagé dans une autre activité (voyage, artisanat ou fouille).

item_place__title = Placer un objet
item_place__select_placeholder = Sélectionnez un objet à placer
item__error = Erreur : {$error}
