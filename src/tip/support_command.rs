use log::log;
use poise::CreateReply;
use serenity::all::{CreateEmbedFooter};
use crate::discord::poise_structs::{Context, Error};

#[poise::command(slash_command, rename = "support")]
pub async fn support_command(ctx: Context<'_>) -> Result<(), Error>{
    match ctx.send(CreateReply::default()
        .content(format!("## {}\n{}", crate::translation::get(ctx, "tips", Some("title"), None), crate::translation::get(ctx, "tips", Some("message"), None)))
        .ephemeral(true),
    )
        .await {
        Ok(_) => {Ok(())}
        Err(e) => {
            log!(log::Level::Error, "failed to reply:\nserver: {:?},\nerror: {:?}", ctx.guild_id(), e);
            Err("reply__reply_failed".into())}
    }
}
