use crate::discord::poise_structs::*;
use crate::provisioning::universe::{create_universe as create_universe_service, NewUniverse};
use crate::provisioning::Provisioner;
use crate::universe::setup::setup_sub_command::SetupType;
use crate::utility::reply::reply;

/// Creates a new universe with the specified name and setup type.
///
/// # Command Permissions
/// - This command requires the user to have `ADMINISTRATOR` permissions.
/// - This command can only be used within a guild (server).
///
/// # Arguments
/// - `ctx`: The command context.
/// - `universe_name`: The name of the universe to be created.
/// - `setup_type`: The type of setup to initialize for the universe. This value is defined by the `SetupType` enum.
///
/// # Returns
/// - `Ok(())` if the universe is created successfully.
/// - `Err(Error)` if an error occurs during the universe creation process.
///
/// # Behavior
/// - The command defers its response to allow for time-consuming operations to be performed without timing out.
/// - The `_create_universe` internal function is called to handle the creation logic.
/// - The result of the operation is sent back as a reply to the user.
///
/// # Examples
/// ```
/// /create_universe universe_name:MyUniverse setup_type:BasicSetup
/// ```
///
/// This will create a new universe named "MyUniverse" with a basic setup.
///
/// # Errors
/// - Errors that occur during the `ctx.defer()` or `reply()` calls will be returned.
/// - Any errors during the `_create_universe` function execution will also be propagated.
///
/// # Notes
/// Ensure that the bot has the necessary permissions and that the command is issued in a valid guild context.
#[poise::command(slash_command, required_permissions= "ADMINISTRATOR", guild_only, rename = "universe_create_universe")]
pub async fn create_universe(
    ctx: Context<'_>,
    universe_name: String,
    setup_type: SetupType
) -> Result<(), Error> {
    let Ok(_) = ctx.defer().await else { return Err("reply__reply_failed".into()) };
    let result = _create_universe(&ctx, universe_name, setup_type).await;
    println!("{:?}", result);
    let Ok(_) = reply(ctx.clone(), result).await else { return Err("reply__reply_failed".into()) };
    Ok(())
}

/// Asynchronously creates a universe and sets it up with initial parameters.
///
/// This function performs several steps to create a new universe:
/// 1. Checks if the universe creation limit for the user has been reached.
/// 2. Validates that no existing universe is associated with the current server.
/// 3. Creates and inserts a new `Universe` into the database.
/// 4. Sets up constraints for the universe.
/// 5. Creates and inserts a `Server` entry associated with the created universe.
/// 6. Creates and inserts a default `Stat` entry (e.g., speed stat).
/// 7. Handles any additional setup as specified by the provided setup type.
///
/// # Parameters
/// * `ctx` - The execution context containing information about the current user and server.
/// * `universe_name` - A `String` specifying the name of the universe to be created.
/// * `setup_type` - An instance of `SetupType` indicating the type of setup to perform (e.g., custom initialization).
///
/// # Returns
/// A `Result` which:
/// - On success: Returns a `&'static str` message indicating successful universe creation.
/// - On failure: Returns an `Error` containing the failure reason.
///
/// # Errors
/// The function may return errors in the following scenarios:
/// - `create_universe__check_universe_limit_failed`:
///   Failed to check the universe limit for the user.
/// - `create_universe__universe_limit_reached`:
///   User has reached the limit for creating universes.
/// - `create_universe__get_server_failed`:
///   Failed to retrieve the server information.
/// - `create_universe__already_exist_for_this_server`:
///   A universe already exists for the current server.
/// - `create_universe__universe_insert_failed`:
///   Failed to insert the created universe into the database.
/// - `create_universe__setup_constraints_failed`:
///   Failed to set up constraints for the created universe.
/// - `create_universe__server_insert_failed`:
///   Failed to insert the server entry for the universe in the database.
/// - `create_universe__speed_stat_insert_failed`:
///   Failed to insert the default speed stat.
/// - Any errors arising from `_setup` when configuring the universe.
///
/// # Example
/// ```rust
/// let result = _create_universe(ctx, "MyUniverse".to_string(), SetupType::Advanced).await;
/// match result {
///     Ok(message) => println!("{}", message),
///     Err(error) => eprintln!("Error creating universe: {:#?}", error),
/// }
/// ```
pub async fn _create_universe(
    ctx: &Context<'_>,
    universe_name: String,
    setup_type: SetupType
) -> Result<&'static str, Error> {
    let locale = ctx.locale().unwrap_or("fr").to_string();
    let mut provisioner = Provisioner::new(ctx.serenity_context().http.clone());
    let new = NewUniverse {
        name: &universe_name,
        creator_id: ctx.author().id.get(),
        guild_id: ctx.guild_id().unwrap().get(),
        setup: setup_type,
        time_modifier: None,
        locale: &locale,
    };
    match create_universe_service(&mut provisioner, new).await {
        Ok(_) => {
            provisioner.finish().await;
            Ok("create_universe__universe_successfully_created")
        }
        // Erreurs de validation (rien n'a été créé) : leur propre message.
        Err(e) if e.key.starts_with("create_universe__") => Err(e.key.into()),
        Err(_) => Err("setup_server__failed".into()),
    }
}
