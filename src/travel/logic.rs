/// # Moteur de Logique de Voyage
/// 
/// Ce module gère le déplacement asynchrone des personnages dans l'univers.
/// Il utilise un système de file d'attente globale (`MOVES`) et une tâche de fond (`SLEEPER`)
/// pour traiter les étapes de voyage de manière efficace sans bloquer le bot.
///
/// **Concepts clés :**
/// - **Segmentation :** Les voyages sont découpés en "steps" (étapes) basées sur la vitesse actuelle.
/// - **Dynamisme :** Si la vitesse d'un membre change (fin d'un buff), le voyage recalcule son temps d'arrivée.
/// - **Multi-serveur :** Gère les invitations automatiques lors des changements de serveurs Discord.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serenity::all::{Http, GuildId, UserId, RoleId, ChannelId, CreateMessage, CreateEmbed, Color};
use anyhow::bail;
use once_cell::sync::Lazy;
use tokio::sync::{Mutex};
use tokio::task::JoinHandle;
use crate::database::travel::{TravelGroup, SpaceType};
use crate::database::characters::get_character_by_user_id;
use chrono::{Local, Timelike, Utc};
use fluent::FluentArgs;
use tokio::time::sleep;
use crate::database::road::get_road_by_channel_id;
use crate::database::stats::{get_stat_by_name, SPEED_STAT};
use crate::tr_locale;
use crate::translation::{get_by_locale};

/// Liste globale des déplacements actifs, triée par temps de fin d'étape (`step_end_timestamp`).
///
/// Cette file est maintenue en permanence par le moteur de voyage. Lorsqu'un groupe de voyage est ajouté
/// ou mis à jour, il est inséré de manière à conserver l'ordre chronologique.
/// Le premier élément de cette liste est toujours celui qui doit expirer le plus tôt.
pub static MOVES: Lazy<Arc<Mutex<Vec<TravelGroup>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));

/// Handle de la tâche asynchrone (Tokio) actuellement en attente du prochain événement de mouvement.
///
/// Le `SLEEPER` contient la tâche `move_process` qui dort pendant la durée nécessaire jusqu'à
/// l'expiration du premier voyage dans `MOVES`.
/// Si un nouveau voyage est ajouté en tête de file, la tâche précédente est annulée (aborted)
/// et un nouveau `SLEEPER` est créé pour refléter le nouveau délai plus court.
pub static SLEEPER: Lazy<Arc<Mutex<Option<JoinHandle<()>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

/// Client HTTP partagé pour les notifications de voyage hors contexte de commande.
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

/// Récupère une URL d'invitation existante ou en crée une nouvelle pour un salon cible.
async fn get_or_create_invite(http: &Arc<Http>, target_guild_id: u64, target_channel_id: ChannelId) -> String {
    let mut invite_url = None;
    let mut server_to_update = None;
    
    if let Ok(Some(target_server)) = crate::database::server::get_server_by_id(target_guild_id).await {
        invite_url = target_server.universal_invite_url.clone();
        server_to_update = Some(target_server);
    }

    if let Some(url) = invite_url {
        return url;
    }

    let mut found_url = None;
    if let Ok(invites) = GuildId::new(target_guild_id).invites(http).await {
        if let Some(invite) = invites.iter().find(|i| i.max_age == 0 && i.max_uses == 0) {
            found_url = Some(invite.url());
        }
    }
    
    if let Some(url) = found_url {
        if let Some(mut s) = server_to_update {
            s.universal_invite_url = Some(url.clone());
            let _ = s.update().await;
        }
        return url;
    }

    if let Ok(invite) = target_channel_id.create_invite(http, serenity::all::CreateInvite::new().max_age(0).max_uses(0).unique(true)).await {
        let url = invite.url();
        if let Some(mut s) = server_to_update {
            s.universal_invite_url = Some(url.clone());
            let _ = s.update().await;
        }
        return url;
    }

    String::new()
}

/// Envoie une invitation de voyage par message privé à un utilisateur.
async fn send_travel_invitation(http: &Arc<Http>, user: &serenity::all::User, user_display_name: &str, universe_id: mongodb::bson::oid::ObjectId, url: &str, target_guild_id: u64) {
    if url.is_empty() { return; }

    // Vérifier si l'utilisateur est déjà sur le serveur cible
    if let Ok(_member) = http.get_member(GuildId::new(target_guild_id), user.id).await {
        println!("User {} is already in guild {}, skipping invitation.", user.id, target_guild_id);
        return;
    }

    let mut universe_name = String::new();
    if let Ok(Some(universe)) = crate::database::universe::get_universe_by_id(universe_id).await {
        universe_name = universe.name;
    }

    let mut args = FluentArgs::new();
    args.set("user", user_display_name);
    args.set("universe", universe_name.as_str());
    args.set("link", url.trim());

    let title = get_by_locale("fr", "travel__invitation", Some("title"), None);
    let description = get_by_locale("fr", "travel__invitation", Some("message"), Some(&args));

    let embed = CreateEmbed::new()
        .title(title)
        .description(description)
        .url(url.trim())
        .color(Color::from_rgb(0, 255, 255));

    let _ = user.direct_message(http, CreateMessage::new().content(url).embed(embed)).await;
}

/// Déclenche une tâche Tokio qui attend la fin de la prochaine étape de voyage.
///
/// **Fonctionnement :**
/// 1. Dort pendant `delay` secondes.
/// 2. À son réveil, elle extrait le premier groupe de `MOVES`.
/// 3. Appelle `next_step_logic` pour calculer l'état suivant du voyage.
/// 4. Si le voyage continue, elle le réinsère dans la file à la bonne position.
/// 5. Si le voyage est terminé (`is_end`), elle gère l'arrivée (messages, rôles Discord).
/// 6. Enfin, elle relance récursivement un nouveau `move_process` pour le prochain élément de la file.
///
/// Cette fonction assure la continuité du mouvement sans boucle `while` bloquante,
/// permettant une gestion efficace des milliers de groupes potentiels.
fn move_process(delay: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        let mut next_delay: Option<u64> = None;
        let mut next_id: Option<String> = None;

        let mut role_updates: Vec<(u64, u64, Option<u64>, Option<u64>)> = Vec::new();

        {
            let mut moves = MOVES.lock().await;
            if moves.is_empty() {
                let mut sleeper = SLEEPER.lock().await;
                *sleeper = None;
                return;
            }

            // Récupère le move qui vient de finir son attente (le premier)
            let current_move = moves.remove(0);

            // Calcule l'étape suivante
            match next_step_logic(&current_move).await {
                Ok(updated_move) => {
                    if current_move.is_end {
                        // Le voyage est totalement fini (le temps d'attente pour l'arrivée est écoulé)
                        let current = Local::now();
                        let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());
                        println!("[{date}] Move for group {} finished", current_move._id);
                        
                        // Envoi des messages de fin de voyage
                        let http_opt = {
                            let lock = HTTP_CLIENT.lock().await;
                            lock.clone()
                        };

                        if let Some(http) = http_opt {
                            let guild_id = current_move.server_id;
                            let members = current_move.members.clone();
                            let road_id = current_move.road_id;
                            let dest_id = current_move.destination_id;
                            let universe_id = current_move.universe_id;
                            let destination_server_id = current_move.destination_server_id;

                            tokio::spawn(async move {
                                let http_arc = http.clone();
                                for &user_id in &members {
                                    if let Ok(user) = http_arc.get_user(UserId::new(user_id)).await {
                                        let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, user_id).await {
                                            char.name
                                        } else {
                                            let member_nick = http_arc.get_member(GuildId::new(guild_id), UserId::new(user_id)).await.ok().and_then(|m| m.nick.clone());
                                            member_nick.unwrap_or(user.name.clone())
                                        };
                                        let user_display_name = character_name;
                                        
                                        // Message dans le salon de la route
                                        if let Some(rid) = road_id {
                                            let mut destination_name = String::new();
                                            if let Some(did) = dest_id {
                                                 if let Ok(Some(place)) = crate::database::places::get_place_by_category_id(universe_id, did).await {
                                                     destination_name = place.name;
                                                 }
                                            }
                                            let msg = tr_locale!("fr", "travel__reached_destination", user: user_display_name.as_str(), destination: destination_name.as_str());
                                            let _ = ChannelId::new(rid).send_message(&http_arc, CreateMessage::new().content(msg)).await;
                                        }

                                        // Message dans le salon de destination
                                        if let Some(did) = dest_id {
                                            let target_guild_id = destination_server_id.unwrap_or(guild_id);

                                            if let Ok(mut channels) = http_arc.get_channels(GuildId::new(target_guild_id)).await {
                                                // On trie les salons par position pour être sûr de prendre le \"premier\"
                                                channels.sort_by_key(|c| c.position);
                                                
                                                // On cherche un salon dans la catégorie de destination, sinon n'importe quel salon textuel
                                                let target_channel = channels.iter().find(|c| c.parent_id == Some(ChannelId::new(did)) && (c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage))
                                                    .or_else(|| channels.iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage));

                                                if let Some(target_channel) = target_channel {
                                                    let msg = tr_locale!("fr", "travel__arrived_at_destination", user: user_display_name.as_str());
                                                    let _ = target_channel.id.send_message(&http_arc, CreateMessage::new().content(msg)).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        }

                        // Déclenche le retrait du rôle de la route et l'ajout du rôle du lieu de destination
                        println!("[{date}] Trip for group {} finished. Destination role: {:?}, Road role: {:?}", current_move._id, current_move.destination_role_id, current_move.road_role_id);
                        
                        // Récupère la route pour savoir sur quel serveur est le salon de la route
                        let road_guild_id = current_move.road_server_id.unwrap_or(current_move.server_id);

                        // La destination finale est sur le serveur spécifié dans le PlayerMove (plus fiable que de recalculer)
                        let target_guild_id = current_move.destination_server_id.unwrap_or(current_move.server_id);

                        for &user_id in &current_move.members {
                            role_updates.push((road_guild_id, user_id, None, current_move.road_role_id));
                            role_updates.push((target_guild_id, user_id, current_move.destination_role_id, None));
                        }
                    } else {
                        // Pas encore fini ou vient de finir une étape, on réinsère
                        let i = moves.partition_point(|a| {
                            a.step_end_timestamp.unwrap_or(0) < updated_move.step_end_timestamp.unwrap_or(0)
                        });
                        moves.insert(i, updated_move);
                    }
                }
                Err(e) => {
                    eprintln!("Error in next_step for group {}: {:?}", current_move._id, e);
                    // On pourrait décider de le remettre ou non, ici on l'abandonne pour éviter les boucles infinies d'erreurs
                }
            }

            // Prépare le prochain sleep si la liste n'est pas vide
            if let Some(first) = moves.first() {
                if let Some(end_ts) = first.step_end_timestamp {
                    let now = Utc::now().timestamp() as u64;
                    next_delay = Some(end_ts.saturating_sub(now));
                    next_id = Some(first._id.to_hex());
                }
            }
        }

        // Relance le processus pour le nouveau premier move (si applicable)
        if let Some(delay) = next_delay {
            let mut sleeper = SLEEPER.lock().await;
            *sleeper = Some(move_process(delay));
            let current = Local::now();
            let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());
            if let Some(id) = next_id {
                println!("[{date}] Next move process started for user {} with delay {}s", id, delay);
            }
        } else {
            let mut sleeper = SLEEPER.lock().await;
            *sleeper = None;
        }

        // Exécute les changements de rôles si nécessaire
        if !role_updates.is_empty() {
            let http_opt = {
                let lock = HTTP_CLIENT.lock().await;
                lock.clone()
            };

            if let Some(http) = http_opt {
                for (guild_id, user_id, add, remove) in role_updates {
                    manage_roles(http.clone(), guild_id, user_id, add, remove).await;
                }
            } else {
                eprintln!("HTTP client not initialized in move_process, cannot update roles");
            }
        }
    })
}


/// Retire un utilisateur (et son groupe) de la liste des déplacements actifs.
pub async fn remove_move(user_id: u64){
    let mut moves = MOVES.lock().await;
    let task_index = moves.iter().position(|a| a.members.contains(&user_id));

    if let Some(player_move_index) = task_index {
        let p_move_id = moves[player_move_index]._id;
        moves.remove(player_move_index);

        let current = Local::now();
        let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());
        println!("[{date}] Move for group {p_move_id} successfully removed");

        if player_move_index == 0 {
            let mut sleeper = SLEEPER.lock().await;
            if let Some(handle) = sleeper.take() {
                handle.abort();
                println!("[{date}] Move for group {p_move_id} task aborted");
            }

            if let Some(next_move) = moves.first() {
                if let Some(end_ts) = next_move.step_end_timestamp {
                    let now = Utc::now().timestamp() as u64;
                    let delay = end_ts.saturating_sub(now);
                    *sleeper = Some(move_process(delay));
                    println!("[{date}] Move for group {} started (new first)", next_move._id);
                }
            }
        }
    } else {
        println!("task not found for user {user_id}");
    }
}

/// Ajoute un nouveau groupe de voyage à la file d'attente globale et réinitialise le `SLEEPER` si nécessaire.
///
/// **Actions :**
/// 1. Verrouille la file `MOVES`.
/// 2. Utilise `partition_point` pour trouver l'index d'insertion correct afin de maintenir le tri chronologique.
/// 3. Insère le groupe dans la file.
/// 4. Si l'élément est inséré à l'index 0 (il devient le plus proche), annule l'ancienne tâche `SLEEPER`
///    et démarre un nouveau `move_process` avec le délai calculé.
pub async fn add_move(player_move: TravelGroup){
    let mut moves = MOVES.lock().await;
    // Récupération de la position où insérer l'étape de déplacement (trié par step_end_timestamp)
    let i = moves.partition_point(|a| {
        a.step_end_timestamp.unwrap_or(0) < player_move.step_end_timestamp.unwrap_or(0)
    });

    moves.insert(i, player_move.clone());

    let current = Local::now();
    let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());
    println!("[{date}] Move for group {} successfully added at index {i}", player_move._id);

    if i == 0 {
        let mut sleeper = SLEEPER.lock().await;
        if let Some(handle) = sleeper.take() {
            handle.abort();
            println!("[{date}] Previous first move task aborted.");
        }

        if let Some(end_ts) = player_move.step_end_timestamp {
            let now = Utc::now().timestamp() as u64;
            let delay = end_ts.saturating_sub(now);
            *sleeper = Some(move_process(delay));
            println!("[{date}] Move for group {} started (new first)", player_move._id);
        }
    }
}


/// Calcule l'état suivant d'un groupe de voyage après une étape de mouvement.
/// 
/// **Logique de calcul :**
/// - Calcule la distance parcourue depuis la dernière étape : `(temps_écoulé * vitesse_précédente)`.
/// - Met à jour `distance_traveled`.
/// - Résout la vitesse actuelle du groupe (la plus petite vitesse parmi tous les membres).
/// - Calcule le "temps de segment" jusqu'au prochain changement de vitesse prévu
///   (ex: fin d'un bonus/malus de vitesse sur un des membres).
/// - Détermine si la distance restante jusqu'à la destination sera parcourue avant la fin de ce segment.
/// - Si oui, marque le voyage comme `is_end` et calcule le timestamp d'arrivée.
/// - Si non, définit le prochain `step_end_timestamp` à la fin du segment actuel.
///
/// Cette segmentation permet de gérer dynamiquement les changements de vitesse en cours de route.
pub async fn next_step_logic(actual_move: &TravelGroup) -> Result<TravelGroup, anyhow::Error> {
    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<TravelGroup, anyhow::Error> = async {
        let mut new_move = actual_move.clone();

        // Si le move est déjà marqué comme terminé, on applique la logique d'arrivée
        if actual_move.is_end {
            println!("[Arrival DB Update] Members: {:?}, DestRole: {:?}, RoadRole: {:?}", new_move.members, new_move.destination_role_id, new_move.road_role_id);
            new_move.actual_space_id = new_move.destination_id.unwrap_or(new_move.actual_space_id);
            new_move.actual_space_type = SpaceType::Place;
            new_move.road_id = None;
            new_move.road_role_id = None;
            new_move.destination_id = None;
            new_move.destination_role_id = None;
            new_move.step_end_timestamp = None;
            new_move.step_start_timestamp = None;
            new_move.is_in_move = false;
            new_move.is_end = false;
            new_move.distance_traveled = 0.0; // Reset distance at arrival

            // Sauvegarde en base de données pour la persistance de l'arrivée
            new_move.upsert_with_session(&mut session).await
                .map_err(|e| anyhow::anyhow!("Failed to update travel group {} in DB at arrival: {:?}", new_move._id, e))?;

            return Ok(new_move);
        }

        // Récupère le stat speed
        let stat_opt = get_stat_by_name(actual_move.universe_id, SPEED_STAT).await?;
        let stat = stat_opt.ok_or_else(|| anyhow::anyhow!("Speed stat not found"))?;

        // Récupère timestamps du step précédent (sécurisé)
        let end_timestamp = new_move.step_end_timestamp.ok_or_else(|| anyhow::anyhow!("step_end_timestamp missing"))?;
        let start_timestamp = new_move.step_start_timestamp.ok_or_else(|| anyhow::anyhow!("step_start_timestamp missing"))?;

        // vitesse utilisée pour le pas précédent (km/h)
        let prev_speed_kmh = new_move.modified_speed;
        if prev_speed_kmh < 0.0 {
            bail!("previous modified speed is negative");
        }

        // travel_time en secondes (peut être 0)
        let travel_time_secs = (end_timestamp as i128 - start_timestamp as i128) as f64;
        let traveled_distance_km = (travel_time_secs / 3600.0) * prev_speed_kmh;
        new_move.distance_traveled += traveled_distance_km;


        // résolution du stat pour obtenir la vitesse actuelle et le modifier le plus court
        let mut final_speed_kmh = f64::MAX;
        let mut shortest_modifier_opt = None;

        for &member_id in &new_move.members {
            let (stat_speed_bson, mod_opt) =
                stat.clone().resolve(actual_move.actual_space_id, member_id).await
                    .map_err(|e| anyhow::anyhow!("stat.resolve error for member {}: {:?}", member_id, e))?;

            let speed = stat_speed_bson.as_f64();
            if speed < final_speed_kmh {
                final_speed_kmh = speed;
            }

            // On cherche le modifier qui se termine le plus tôt parmi tous les membres
            if let Some(m) = mod_opt {
                if let Some(end_ts) = m.end_timestamp {
                    if let Some(current_shortest) = &shortest_modifier_opt {
                        if let Some(current_end_ts) = (current_shortest as &crate::database::modifiers::Modifier).end_timestamp {
                            if end_ts < current_end_ts {
                                shortest_modifier_opt = Some(m);
                            }
                        } else {
                            shortest_modifier_opt = Some(m);
                        }
                    } else {
                        shortest_modifier_opt = Some(m);
                    }
                }
            }
        }

        if final_speed_kmh == f64::MAX {
            bail!("No members in travel group or speed could not be determined");
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // récupère road
        let road_opt = get_road_by_channel_id(actual_move.universe_id, actual_move.road_id.ok_or_else(|| anyhow::anyhow!("road_id missing"))?).await?;
        let road = road_opt.ok_or_else(|| anyhow::anyhow!("road not found"))?;

        if final_speed_kmh <= 0.0 {
            bail!("final_speed must be > 0");
        }

        // remaining distance en km
        let road_distance_km = road.distance as f64;
        let remaining_distance_km = (road_distance_km - new_move.distance_traveled).max(0.0);

        // si on est déjà arrivé
        if remaining_distance_km <= std::f64::EPSILON {
            new_move.is_end = true;
            new_move.step_start_timestamp = None;
            new_move.step_end_timestamp = None;
            return Ok(new_move);
        }

        // temps total nécessaire pour finir (secondes)
        // remaining_distance_km / final_speed_kmh => heures ; *3600 => secondes
        let time_needed_secs_f = (remaining_distance_km / final_speed_kmh) * 3600.0;
        let full_time_needed_secs = time_needed_secs_f.ceil() as u64;

        println!("[Travel Debug] Group: {}, Speed: {:.2} km/h, Remaining Time: {}s (Dist: {:.2}km)",
                 new_move._id, final_speed_kmh, full_time_needed_secs, remaining_distance_km);

        // clamp par le shortest_modifier s'il existe (modifier.end_timestamp est un timestamp absolu)
        let mut time_to_wait_secs = full_time_needed_secs;

        if let Some(shortest_modifier) = shortest_modifier_opt {
            if let Some(mod_end_ts) = shortest_modifier.end_timestamp {
                // si le modifier se termine avant now -> il est expiré, pas d'effet
                if mod_end_ts > now {
                    let remaining_modifier_secs = mod_end_ts - now;
                    if remaining_modifier_secs < time_to_wait_secs {
                        time_to_wait_secs = remaining_modifier_secs;
                    }
                }
            }
        }

        // si time_to_wait >= full_time_needed => on arrivera au bout de la route
        if time_to_wait_secs >= full_time_needed_secs {
            new_move.is_end = true;
        } else {
            new_move.is_end = false;
        }

        // met à jour timestamps et speed
        new_move.step_start_timestamp = Some(now);
        new_move.step_end_timestamp = Some(now + time_to_wait_secs);
        new_move.modified_speed = final_speed_kmh;

        // Sauvegarde en base de données pour la persistance
        new_move.upsert_with_session(&mut session).await
            .map_err(|e| anyhow::anyhow!("Failed to update travel group {} in DB: {:?}", new_move._id, e))?;

        Ok(new_move)
    }.await;

    match result {
        Ok(val) => {
            session.commit_transaction().await?;
            Ok(val)
        }
        Err(e) => {
            session.abort_transaction().await?;
            Err(e)
        }
    }
}

/// Initialise le système de voyage au démarrage du bot.
/// 
/// Recharge tous les voyages actifs depuis la base de données et relance les processus.
#[allow(dead_code)]
pub async fn setup(){
    let universes = match crate::database::universe::Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Failed to get universes for travel setup: {:?}", e);
            return;
        }
    };

    let mut all_moves = Vec::new();

    for universe in universes {
        match crate::database::travel::TravelGroup::get_active_moves(universe.universe_id).await {
            Ok(moves) => all_moves.extend(moves),
            Err(e) => eprintln!("Failed to get active moves for universe {}: {:?}", universe.universe_id, e),
        }
    }

    if all_moves.is_empty() {
        println!("Travel system initialized: 0 active moves.");
        return;
    }

    let now = Utc::now().timestamp() as u64;
    let mut pending_moves = Vec::new();
    let mut ready_to_process = VecDeque::new();

    for m in all_moves {
        if let Some(end_ts) = m.step_end_timestamp {
            if end_ts <= now {
                ready_to_process.push_back(m);
            } else {
                pending_moves.push(m);
            }
        } else {
            // Un mouvement actif sans timestamp est anormal, on le traite pour voir s'il peut être réparé ou terminé
            ready_to_process.push_back(m);
        }
    }

    // On traite immédiatement ceux qui sont déjà finis ou en retard
    while let Some(m) = ready_to_process.pop_front() {
        match next_step_logic(&m).await {
            Ok(updated) => {
                if updated.is_in_move {
                    // Si le mouvement continue (nouvelle étape), on vérifie si cette nouvelle étape est aussi déjà passée
                    if let Some(new_end) = updated.step_end_timestamp {
                        if new_end <= now {
                            ready_to_process.push_back(updated);
                        } else {
                            pending_moves.push(updated);
                        }
                    } else {
                        // Devrait pas arriver pour un move en cours
                        pending_moves.push(updated);
                    }
                } else {
                    // Voyage terminé, on gère les rôles et messages si c'était une arrivée
                    if m.is_end {
                        let current = Local::now();
                        let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());
                        println!("[{date}] Recovered move for group {} finished during setup", m._id);
                        
                        let http_opt = {
                            let lock = HTTP_CLIENT.lock().await;
                            lock.clone()
                        };

                        if let Some(http) = http_opt {
                            let guild_id = m.server_id;
                            let road_id = m.road_id;
                            let dest_id = m.destination_id;
                            let universe_id = m.universe_id;
                            let dest_role = m.destination_role_id;
                            let road_role = m.road_role_id;
                            let members = m.members.clone();
                            let destination_server_id = m.destination_server_id;

                            tokio::spawn(async move {
                                let http_arc = http.clone();
                                for &user_id in &members {
                                    if let Ok(user) = http_arc.get_user(UserId::new(user_id)).await {
                                        let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, user_id).await {
                                            char.name
                                        } else {
                                            let member_nick = http_arc.get_member(GuildId::new(guild_id), UserId::new(user_id)).await.ok().and_then(|m| m.nick.clone());
                                            member_nick.unwrap_or(user.name.clone())
                                        };
                                        let user_display_name = character_name;
                                        
                                        // Message dans le salon de la route
                                        if let Some(rid) = road_id {
                                            let mut destination_name = String::new();
                                            if let Some(did) = dest_id {
                                                 if let Ok(Some(place)) = crate::database::places::get_place_by_category_id(universe_id, did).await {
                                                     destination_name = place.name;
                                                 }
                                            }
                                            let msg = tr_locale!("fr", "travel__reached_destination", user: user_display_name.as_str(), destination: destination_name.as_str());
                                            let _ = ChannelId::new(rid).send_message(&http_arc, CreateMessage::new().content(msg)).await;
                                        }

                                        // Message dans le salon de destination
                                        if let Some(did) = dest_id {
                                            let target_guild_id = destination_server_id.unwrap_or(guild_id);

                                            if let Ok(mut channels) = http_arc.get_channels(GuildId::new(target_guild_id)).await {
                                                channels.sort_by_key(|c| c.position);
                                                
                                                let target_channel = channels.iter().find(|c| c.parent_id == Some(ChannelId::new(did)) && (c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage))
                                                    .or_else(|| channels.iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage));

                                                if let Some(target_channel) = target_channel {
                                                    let msg = tr_locale!("fr", "travel__arrived_at_destination", user: user_display_name.as_str());
                                                    let _ = target_channel.id.send_message(&http_arc, CreateMessage::new().content(msg)).await;
                                                }
                                            }
                                        }
                                    }
                                    // Gestion des rôles pour chaque membre
                                    manage_roles(http_arc.clone(), destination_server_id.unwrap_or(guild_id), user_id, dest_role, None).await;
                                    manage_roles(http_arc.clone(), guild_id, user_id, None, road_role).await;
                                }
                            });
                        }
                    }
                }
            }
            Err(e) => eprintln!("Failed to process recovered move for group {}: {:?}", m._id, e),
        }
    }

    if pending_moves.is_empty() {
        println!("Travel system initialized: 0 pending moves (all recovered were processed).");
        return;
    }

    // Tri par step_end_timestamp croissant
    pending_moves.sort_by_key(|m| m.step_end_timestamp.unwrap_or(0));

    {
        let mut moves = MOVES.lock().await;
        *moves = pending_moves;

        // On lance le sleeper pour le premier move
        if let Some(first) = moves.first() {
            if let Some(end_ts) = first.step_end_timestamp {
                let delay = end_ts.saturating_sub(now);
                let mut sleeper = SLEEPER.lock().await;
                *sleeper = Some(move_process(delay));
                println!("Travel system initialized: {} active moves, next in {}s", moves.len(), delay);
            }
        }
    }
}

/// Ajoute ou retire un rôle Discord à un utilisateur.
pub async fn manage_roles(http: Arc<Http>, guild_id: u64, user_id: u64, role_to_add: Option<u64>, role_to_remove: Option<u64>) {
    let guild_id_obj = GuildId::new(guild_id);
    let user_id_obj = UserId::new(user_id);
    let current = Local::now();
    let date = format!("{:02}:{:02}:{:02}", current.hour(), current.minute(), current.second());

    // Vérifier si l'utilisateur est présent sur le serveur
    let member = match http.get_member(guild_id_obj, user_id_obj).await {
        Ok(m) => m,
        Err(_) => {
            println!("[{date}] User {} not found on guild {}, skipping role updates.", user_id, guild_id);
            return;
        }
    };

    // Récupérer les rôles du serveur pour vérifier l'existence
    let guild_roles = match http.get_guild_roles(guild_id_obj).await {
        Ok(roles) => roles,
        Err(_) => {
            eprintln!("[{date}] Failed to fetch roles for guild {}, skipping role updates.", guild_id);
            return;
        }
    };

    if let Some(role_id) = role_to_remove {
        if guild_roles.iter().any(|r| r.id.get() == role_id) {
            if member.roles.iter().any(|r| r.get() == role_id) {
                println!("[{date}] Removing role {} from user {} on guild {}", role_id, user_id, guild_id);
                if let Err(e) = http.remove_member_role(guild_id_obj, user_id_obj, RoleId::new(role_id), None).await {
                    eprintln!("[{date}] Failed to remove role {} from member {} on guild {}: {:?}", role_id, user_id, guild_id, e);
                }
            }
        } else {
            println!("[{date}] Role {} does not exist on guild {}, skipping removal.", role_id, guild_id);
        }
    }

    if let Some(role_id) = role_to_add {
        if guild_roles.iter().any(|r| r.id.get() == role_id) {
            if !member.roles.iter().any(|r| r.get() == role_id) {
                println!("[{date}] Adding role {} to user {} on guild {}", role_id, user_id, guild_id);
                if let Err(e) = http.add_member_role(guild_id_obj, user_id_obj, RoleId::new(role_id), None).await {
                    eprintln!("[{date}] Failed to add role {} to member {} on guild {}: {:?}", role_id, user_id, guild_id, e);
                }
            }
        } else {
            println!("[{date}] Role {} does not exist on guild {}, skipping addition.", role_id, guild_id);
        }
    }
}

/// Enregistre un nouveau voyage pour un groupe de joueurs et initialise le mouvement.
/// 
/// **Étapes clés :**
/// 1. **Validation :** Vérifie qu'aucun membre n'est en train de crafter.
/// 2. **Initialisation :** Définit les flags `is_in_move`, `is_end`, et les timestamps de départ.
/// 3. **Gestion des Rôles :** Retire le rôle du lieu de départ pour tous les membres.
/// 4. **Multi-serveur :** Si la route se trouve sur un autre serveur Discord :
///    - Identifie le salon cible.
///    - Crée ou récupère une invitation permanente.
///    - Envoie l'invitation en DM à chaque membre du groupe.
/// 5. **Mise en file :** Appelle `add_move` pour insérer le voyage dans le moteur asynchrone.
pub async fn add_travel(http: Arc<Http>, guild_id: u64, mut player_move: TravelGroup) -> Result<(), anyhow::Error> {
    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<TravelGroup, anyhow::Error> = async {
        // Vérifier si un craft est en cours pour l'un des membres
        for &user_id in &player_move.members {
            if let Ok(Some(_)) = crate::database::craft::PlayerCraft::get_by_user_id(player_move.universe_id, user_id).await {
                return Err(anyhow::anyhow!("travel__cannot_move_while_crafting"));
            }
        }

        // Initialise les flags de base
        player_move.is_in_move = true;
        player_move.is_end = false;

        // On s'assure que l'ID est valide
        if player_move._id.to_hex() == "000000000000000000000000" {
            player_move._id = mongodb::bson::oid::ObjectId::new();
        }

        // On initialise les timestamps pour le calcul initial (step fictif fini à 'now')
        // seulement si ce n'est pas déjà un voyage en cours (ex: demi-tour)
        let now = Utc::now().timestamp() as u64;
        if player_move.step_start_timestamp.is_none() || player_move.distance_traveled == 0.0 {
            player_move.step_start_timestamp = Some(now);
            player_move.step_end_timestamp = Some(now);
            player_move.modified_speed = 0.0;
        }

        // Déterminer le serveur cible au cas où la route commence sur un autre serveur
        let mut start_guild_id = guild_id;
        if let Some(road_id) = player_move.road_id {
            if let Ok(Some(road)) = crate::database::road::get_road_by_channel_id(player_move.universe_id, road_id).await {
                 start_guild_id = road.server_id;
            }
        }

        // On upsert en base pour que l'appel à upsert() dans next_step_logic fonctionne
        player_move.server_id = start_guild_id;
        if player_move.road_server_id.is_none() {
            player_move.road_server_id = Some(start_guild_id);
        }
        player_move.upsert_with_session(&mut session).await?;

        Ok(player_move)
    }.await;

    let player_move = match result {
        Ok(pm) => {
            session.commit_transaction().await?;
            pm
        }
        Err(e) => {
            session.abort_transaction().await?;
            return Err(e);
        }
    };

    let start_guild_id = player_move.server_id;

    // Gestion des rôles au début du voyage pour tous les membres
    for &user_id in &player_move.members {
        if let Some(source_server) = player_move.source_server_id {
            manage_roles(http.clone(), source_server, user_id, None, player_move.source_role_id).await;
        } else {
            manage_roles(http.clone(), guild_id, user_id, None, player_move.source_role_id).await;
        }
    }
    
    // Si la route est sur un serveur distant, envoyer une invitation à tous les membres
    if start_guild_id != guild_id {
        let http_arc = http.clone();
        let members = player_move.members.clone();
        let road_id = player_move.road_id;
        let universe_id = player_move.universe_id;
        
        println!("[Invitation Debug] Road server {} is different from current server {}. Attempting to send road invitations.", start_guild_id, guild_id);
        
        tokio::spawn(async move {
            if let Some(rid) = road_id {
                for &user_id in &members {
                    if let Ok(user) = http_arc.get_user(UserId::new(user_id)).await {
                        if let Ok(channels) = http_arc.get_channels(GuildId::new(start_guild_id)).await {
                            // On cherche le salon de la route directement par son ID
                            let target_channel = channels.iter().find(|c| c.id.get() == rid)
                                .or_else(|| {
                                    // Fallback: premier salon textuel
                                    let mut sorted_channels: Vec<serenity::all::GuildChannel> = channels.clone();
                                    sorted_channels.sort_by_key(|c| c.position);
                                    None
                                });

                            // Si on n'a pas trouvé par ID, on cherche dans la liste originale pour le fallback
                            let target_channel = if target_channel.is_none() {
                                let mut sorted_channels = channels.clone();
                                sorted_channels.sort_by_key(|c| c.position);
                                sorted_channels.into_iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage)
                            } else {
                                target_channel.cloned()
                            };

                            if let Some(target_channel) = target_channel {
                                 println!("[Invitation Debug] Found channel {} on road server {}. Creating invite.", target_channel.id, start_guild_id);
                                 let url = get_or_create_invite(&http_arc, start_guild_id, target_channel.id).await;
                                 
                                 let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, user_id).await {
                                     char.name
                                 } else {
                                     let member_nick = http_arc.get_member(GuildId::new(guild_id), UserId::new(user_id)).await.ok().and_then(|m| m.nick.clone());
                                     member_nick.unwrap_or(user.name.clone())
                                 };
                                 let user_display_name = character_name;
                                 
                                 send_travel_invitation(&http_arc, &user, &user_display_name, universe_id, &url, start_guild_id).await;
                            } else {
                                 println!("[Invitation Debug] No suitable channel found on road server {} for road invitation.", start_guild_id);
                            }
                        } else {
                            println!("[Invitation Debug] Failed to fetch channels for road server {}.", start_guild_id);
                        }
                    }
                }
            }
        });
    }

    // Si la destination est sur un serveur distant, envoyer une invitation également dès le début à tous les membres
    if let Some(dest_guild_id) = player_move.destination_server_id {
        if dest_guild_id != guild_id && Some(dest_guild_id) != player_move.road_server_id {
            let http_arc = http.clone();
            let members = player_move.members.clone();
            let dest_id = player_move.destination_id;
            let universe_id = player_move.universe_id;
            
            println!("[Invitation Debug] Destination server {} is different from current server {} and road server. Attempting to send destination invitations.", dest_guild_id, guild_id);
            
            tokio::spawn(async move {
                for &user_id in &members {
                    if let Ok(user) = http_arc.get_user(UserId::new(user_id)).await {
                        if let Ok(channels) = http_arc.get_channels(GuildId::new(dest_guild_id)).await {
                             // On cherche un salon dans la catégorie de destination, sinon n'importe quel salon textuel
                             let target_channel = if let Some(did) = dest_id {
                                 channels.iter().find(|c| c.parent_id == Some(ChannelId::new(did)) && (c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage))
                                     .or_else(|| {
                                         let mut sorted_channels: Vec<serenity::all::GuildChannel> = channels.clone();
                                         sorted_channels.sort_by_key(|c| c.position);
                                         None
                                     })
                             } else {
                                 None
                             };

                             // Si on n'a pas trouvé par ID, on cherche dans la liste originale pour le fallback
                             let target_channel = if target_channel.is_none() {
                                 let mut sorted_channels = channels.clone();
                                 sorted_channels.sort_by_key(|c| c.position);
                                 sorted_channels.into_iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage)
                             } else {
                                 target_channel.cloned()
                             };

                             if let Some(target_channel) = target_channel {
                                 println!("[Invitation Debug] Found channel {} on destination server {}. Creating invite.", target_channel.id, dest_guild_id);
                                 let url = get_or_create_invite(&http_arc, dest_guild_id, target_channel.id).await;
                                 
                                 let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, user_id).await {
                                     char.name
                                 } else {
                                     let member_nick = http_arc.get_member(GuildId::new(guild_id), UserId::new(user_id)).await.ok().and_then(|m| m.nick.clone());
                                     member_nick.unwrap_or(user.name.clone())
                                 };
                                 let user_display_name = character_name;
                                 
                                 send_travel_invitation(&http_arc, &user, &user_display_name, universe_id, &url, dest_guild_id).await;
                             }
                        }
                    }
                }
            });
        }
    }

    let road_guild_id = player_move.road_server_id.unwrap_or(start_guild_id);
    for &user_id in &player_move.members {
        manage_roles(http.clone(), road_guild_id, user_id, player_move.road_role_id, None).await;
    }

    // Calcule la première étape réelle (avec la vraie vitesse et le premier modifier)
    let first_step = next_step_logic(&player_move).await?;

    // Envoi du message de début de voyage dans le salon de la route
    if let Some(road_id) = first_step.road_id {
        let http_clone = http.clone();
        let universe_id = first_step.universe_id;
        let dest_id = first_step.destination_id;
        let first_step_members = first_step.members.clone();
                
        tokio::spawn(async move {
            for &user_id in &first_step_members {
                if let Ok(user) = http_clone.get_user(UserId::new(user_id)).await {
                    let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, user_id).await {
                        char.name
                    } else {
                        let member_nick = http_clone.get_member(GuildId::new(guild_id), UserId::new(user_id)).await.ok().and_then(|m| m.nick.clone());
                        member_nick.unwrap_or(user.name.clone())
                    };
                    let user_display_name = character_name;

                    let mut destination_name = String::new();
                    let mut is_secret = false;
                    if let Some(rid) = first_step.road_id {
                        if let Ok(Some(road)) = crate::database::road::get_road_by_channel_id(universe_id, rid).await {
                            is_secret = road.secret;
                        }
                    }

                    if let Some(did) = dest_id {
                        if let Ok(Some(place)) = crate::database::places::get_place_by_category_id(universe_id, did).await {
                            destination_name = place.name;
                        }
                    }
                    
                    let msg = tr_locale!("fr", "travel__moving_to_place", user: user_display_name.as_str(), destination: destination_name.as_str());
                    let _ = ChannelId::new(road_id).send_message(&http_clone, CreateMessage::new().content(msg.clone())).await;

                    // Envoi du message dans le lieu de départ si applicable
                    if let Some(source_id) = first_step.source_id {
                        let source_guild_id = first_step.source_server_id.unwrap_or(guild_id);
                        if let Ok(mut channels) = http_clone.get_channels(GuildId::new(source_guild_id)).await {
                            channels.sort_by_key(|c| c.position);
                            let target_channel = channels.iter().find(|c| c.parent_id == Some(ChannelId::new(source_id)) && (c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage))
                                .or_else(|| channels.iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage));

                            if let Some(target_channel) = target_channel {
                                let departure_msg = if is_secret {
                                    tr_locale!("fr", "travel__taking_unknown_road", user: user_display_name.as_str())
                                } else {
                                    msg
                                };
                                let _ = target_channel.id.send_message(&http_clone, CreateMessage::new().content(departure_msg)).await;
                            }
                        }
                    }
                }
            }
        });
    }

    // Ajoute à la file active (et lance/met à jour le sleeper si nécessaire)
    add_move(first_step).await;
    Ok(())
}

/// Retire un voyage de la file active pour un utilisateur donné.
#[allow(dead_code)]
pub async fn remove_travel(user_id: u64) {
    remove_move(user_id).await;
}

/// Calcule le seuil de proximité autorisé pour rejoindre ou estimer, basé sur le modificateur de temps.
pub fn get_travel_threshold(modifier: u64) -> f64 {
    100.0 + (modifier as f64 / 10.0)
}

/// Calcule la distance parcourue réelle (en km) d'un groupe à l'instant T.
pub fn calculate_current_distance(player_move: &TravelGroup) -> f64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let start_ts = player_move.step_start_timestamp.unwrap_or(now);
    let end_ts = player_move.step_end_timestamp.unwrap_or(now);
    
    // Si on est déjà après la fin du step, on est censé avoir parcouru toute la distance du step
    let effective_now = now.min(end_ts);
    let elapsed_secs = (effective_now.saturating_sub(start_ts)) as f64;
    let speed_kmh = player_move.modified_speed;
    
    let step_distance = (elapsed_secs / 3600.0) * speed_kmh;
    player_move.distance_traveled + step_distance
}

/// Arrête immédiatement le voyage d'un groupe et met à jour sa position finale.
pub async fn stop_travel(user_id: u64) -> Result<TravelGroup, anyhow::Error> {
    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<TravelGroup, anyhow::Error> = async {
        let moves_lock = MOVES.lock().await;
        let index = moves_lock.iter().position(|m| m.members.contains(&user_id));

        let mut player_move = if let Some(idx) = index {
            moves_lock[idx].clone()
        } else {
            return Err(anyhow::anyhow!("No active move found for user {}", user_id));
        };

        // Calculer la distance actuelle avant de l'arrêter
        player_move.distance_traveled = calculate_current_distance(&player_move);
        player_move.is_in_move = false;
        player_move.step_start_timestamp = None;
        player_move.step_end_timestamp = None;

        // Sauvegarder en DB
        player_move.upsert_with_session(&mut session).await
            .map_err(|e| anyhow::anyhow!("DB error: {:?}", e))?;

        Ok(player_move)
    }.await;

    let player_move = match result {
        Ok(pm) => {
            session.commit_transaction().await?;
            pm
        }
        Err(e) => {
            session.abort_transaction().await?;
            return Err(e);
        }
    };

    remove_move(user_id).await;

    // Envoi du message d'interruption dans le salon de la route pour chaque membre
    if let Some(road_id) = player_move.road_id {
        if let Some(http) = HTTP_CLIENT.lock().await.clone() {
            let universe_id = player_move.universe_id;
            let members = player_move.members.clone();
            let guild_id = player_move.server_id;

            tokio::spawn(async move {
                for &m_id in &members {
                    let character_name = if let Ok(Some(char)) = get_character_by_user_id(universe_id, m_id).await {
                        char.name
                    } else {
                        let member_nick = http.get_member(GuildId::new(guild_id), UserId::new(m_id)).await.ok().and_then(|m| m.nick.clone());
                        if let Some(nick) = member_nick {
                            nick
                        } else if let Ok(user) = http.get_user(UserId::new(m_id)).await {
                            user.name
                        } else {
                            format!("User {}", m_id)
                        }
                    };

                    let msg = tr_locale!("fr", "travel__interrupted", user: character_name);
                    let _ = ChannelId::new(road_id).send_message(&http, CreateMessage::new().content(msg)).await;
                }
            });
        }
    }

    Ok(player_move)
}