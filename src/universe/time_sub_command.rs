use crate::discord::poise_structs::{Context, Error};
use crate::database::universe::{get_universe_by_server_id};
use crate::universe::time::TimePhase;
use chrono::Utc;
use crate::utility::reply::reply_with_args;
use fluent::FluentArgs;

#[poise::command(slash_command, rename = "universe_time", required_permissions = "ADMINISTRATOR", guild_only)]
pub async fn time(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().get();
    
    let universe = match get_universe_by_server_id(guild_id).await {
        Ok(Some(u)) => u,
        _ => return Err("travel__server_not_found".into()),
    };

    let modifier = universe.global_time_modifier as f64 / 100.0;
    if modifier <= 0.0 {
        return Err("universe_time__invalid_modifier".into());
    }

    let phase_duration_secs = (21600.0 / modifier) as u64;
    let cycle_duration_secs = phase_duration_secs * 4;

    let now_secs = Utc::now().timestamp() as u64;
    let origin_secs = (universe.time_origin_timestamp / 1000) as u64;
    let elapsed_secs = now_secs.saturating_sub(origin_secs);
    
    let current_cycle_pos = elapsed_secs % cycle_duration_secs;
    let current_phase_idx = current_cycle_pos / phase_duration_secs;
    
    let phase = TimePhase::from_index(current_phase_idx);
    
    // Calcul de l'heure RP (00:00 à 23:59)
    // Le cycle de 24h RP correspond à cycle_duration_secs réels.
    let rp_total_seconds = (current_cycle_pos as f64 / cycle_duration_secs as f64) * 86400.0;
    let rp_hours = (rp_total_seconds / 3600.0) as u32;
    let rp_minutes = ((rp_total_seconds % 3600.0) / 60.0) as u32;
    
    let rp_time_str = format!("{:02}:{:02}", rp_hours, rp_minutes);
    let phase_name = crate::translation::get(ctx, phase.get_message_key(), None, None);

    let mut args = FluentArgs::new();
    args.set("time", rp_time_str);
    args.set("phase", phase_name);

    let _ = reply_with_args(ctx, Ok("universe_time__current_time"), Some(args)).await;

    Ok(())
}
