use crate::discord::poise_structs::{Context, Error};
use crate::database::universe::{get_universe_by_server_id};
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

    let Some((origin_secs, phase_duration_secs)) = crate::universe::time::universe_time_params(&universe) else {
        return Err("universe_time__invalid_modifier".into());
    };
    // Même calcul que l'horloge de l'activité.
    let (rp_hours, rp_minutes, phase) =
        crate::universe::time::time_of_day(Utc::now().timestamp() as u64, origin_secs, phase_duration_secs);

    let rp_time_str = format!("{:02}:{:02}", rp_hours, rp_minutes);
    let phase_name = crate::translation::get(ctx, phase.get_message_key(), None, None);

    let mut args = FluentArgs::new();
    args.set("time", rp_time_str);
    args.set("phase", phase_name);

    let _ = reply_with_args(ctx, Ok("universe_time__current_time"), Some(args)).await;

    Ok(())
}
