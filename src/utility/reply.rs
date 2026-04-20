use log::log;
use serenity::all::{Color, CreateEmbed, CreateEmbedFooter};
use crate::discord::poise_structs::{Context, Error};
use poise::CreateReply;
use fluent::FluentArgs;

/// Sends an embed-based reply to a user based on the result provided, with appropriate styling
/// (green for success and red for failure) and localized content.
///
/// # Parameters
/// - `ctx`: The context of the interaction, which provides necessary metadata such as the guild ID
///   and functionality for sending replies.
/// - `result`: A `Result` containing either a success message (`Ok`) or an error (`Err`) to be
///   used for constructing the reply embed.
///
/// # Returns
/// - On success, returns `Ok("reply__reply_success")`.
/// - On failure, returns `Err("reply__reply_failed")`.
///
/// # Behavior
/// 1. The function determines the outcome (`Ok` or `Err`) from the `result` parameter and extracts 
///    the corresponding message. Based on the result:
///    - A success case generates a green embed with the success message.
///    - A failure case generates a red embed with the error message.
/// 2. The embed includes:
///    - A localized title (`title`) and message (`message`) retrieved using the 
///      `crate::translation::get` function.
///    - A footer that displays the original string message.
///    - A color indicating the status (green for success, red for failure).
/// 3. Attempts to send the constructed embed using the `ctx.send` function. If sending succeeds,
///    the function returns `Ok("reply__reply_success")`.
/// 4. Logs an error and returns `Err("reply__reply_failed")` when the sending fails. The log entry
///    includes details about the server (`guild_id`) and the error message.
///
/// # Examples
/// ```rust
/// let ctx = /* some context */;
/// let result: Result<&str, Error> = Ok("Operation successful");
///
/// let response = reply(ctx, result).await;
/// match response {
///     Ok(success_message) => println!("{}", success_message), // "reply__reply_success"
///     Err(error_message) => println!("{}", error_message),   // "reply__reply_failed"
/// }
/// ```
///
/// # Errors
/// - If the process of sending the reply via `ctx.send` fails, the function logs the issue and
///   returns an appropriate error message wrapped in `Err`.
///
/// # Notes
/// - The `crate::translation::get` function is used to fetch localized strings for the embed's
///   title and description based on the message content. Ensure that the translation keys exist
///   and are properly configured.
/// - The embed's color uses RGB values to visually indicate success or failure.
pub async fn reply<'a>(
    ctx: Context<'a>,
    result: Result<&'a str, Error>,
) -> Result<&'a str, Error> {
    reply_with_args(ctx, result, None).await
}

pub async fn reply_with_args<'a>(
    ctx: Context<'a>,
    result: Result<&'a str, Error>,
    args: Option<FluentArgs<'a>>,
) -> Result<&'a str, Error> {
    let ephemeral = result.is_err();
    reply_with_args_and_ephemeral(ctx, result, args, ephemeral).await
}

pub async fn reply_with_args_and_ephemeral<'a>(
    ctx: Context<'a>,
    result: Result<&'a str, Error>,
    args: Option<FluentArgs<'a>>,
    ephemeral: bool,
) -> Result<&'a str, Error> {
    let (color, string) = match &result {
        Ok(string) => (Color::from_rgb(0, 255, 0), string.to_string()),
        Err(error) => (Color::from_rgb(255, 0, 0), error.to_string()),
    };

    let (id, final_args) = if string.starts_with("error:") {
        let parts: Vec<&str> = string.splitn(3, ':').collect();
        if parts.len() == 3 {
            let key = parts[1].to_string();
            let err_msg = parts[2].to_string();
            let mut new_args = args.unwrap_or_else(|| FluentArgs::new());
            new_args.set("error", err_msg);
            (key, Some(new_args))
        } else {
            (string.clone(), args)
        }
    } else {
        (string.clone(), args)
    };

    match ctx.send(CreateReply::default().embed(
            CreateEmbed::new()
                .title(crate::translation::smart_tr(ctx, &format!("{}.title", id), final_args.as_ref()).unwrap_or_else(|_| crate::translation::smart_tr(ctx, &id, final_args.as_ref()).unwrap_or_else(|_| id.clone())))
                .description(crate::translation::smart_tr(ctx, &format!("{}.message", id), final_args.as_ref()).unwrap_or_else(|_| id.clone()))
                .footer(CreateEmbedFooter::new(id.clone()))
                .color(color),
        ).ephemeral(ephemeral),
    )
        .await {
        Ok(_) => {Ok("reply__reply_success")}
        Err(e) => {
            log!(log::Level::Error, "failed to reply:\nserver: {:?}\nerror_string: {}\nerror: {:?}", ctx.guild_id(), string, e);
            Err("reply__reply_failed".into())}
    }
}