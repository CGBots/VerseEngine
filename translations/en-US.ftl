botname = VerseEngine

placeholder = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.
    .title = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.
    .message = Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla eget neque arcu. Integer sed turpis.

support = contact.cgbots@gmail.com or @cgbots on discord

tips = Support the project
    .title = Support the project
    .message = Thanks to support the project ! You can do a tip at this page: https://ko-fi.com/cgbot

start_message = Start Message
    .title = Thank you for using {botname}
    .description = To start using the bot, begin by creating a new universe.
            Use the command `/{universe} {create_universe} [your universe name] [setup type]`
            The setup type determines which channels will be created.
            In a partial setup, only the road category and roles will be created.
            In a full setup, the Admin, out of rp, rp categories and their channels are also created.

carousel__previous_button = Previous
carousel__next_button = Next
carousel__refresh_button = Refresh

#Loot
loot = loot
    .description = Actions related to searching and looting areas
loot_search = search
    .description = Start searching the current area
loot_stop = stop
    .description = Stop the current search

#Loot tables
loot_table = loot_table
    .description = Manage loot tables
loot_table_edit = edit
    .description = Create or edit a loot table for a channel
    .channel_id = channel_id
    .channel_id-description = Channel ID (Place category, Road channel, or Place's sub-channel)
loot_table__modal_title = Loot Table Editor
loot_table__modal_field_name = Loot Table Content
loot_table__modal_placeholder = # Loot Table Syntax Guide
    - Loot Tables can contain two types of elements: **items** and **sets**.
    ## Items
    ```
    ­[item name]:[probability in %], [min-max range], stock:[number], secret
    ```
    - Item names are used to identify objects and are case-sensitive.
    - The probability of each item is **absolute** and given in % implicitly (no need to specify %): `40`
    - Each line in the loot table has a chance to be rolled independently of others. The sum of probabilities can therefore exceed 100%.
    - When a looted item has a range, the number of items obtained corresponds to a random number chosen within that range.
    - The `min-max` range can be replaced by a single number if min and max are identical: `2-2` becomes `2`
    - __Optional__: If the loot is in limited quantity, the stock parameter can be specified: `stock:10`
    - __Optional__: The `secret` keyword can be used so that the item does not appear in loot tables in the wiki.
    - If a stock drops to 0, items are removed from the table and a message is sent to the log channel.
    ~~-----------------------------------------------------------------~~
    ## Sets
    ```
    ­[set name]:[probability in %], [min-max], stock:[number], secret
    - [item name]:[weight], [min-max], stock:[number], secret
    - ...
    ```
    - The set declaration is identical to that of an item.
    - Set names are purely technical and will not be displayed in the wiki.
    - Sets contain a list of items.
    - Items in sets have the same syntax as standalone items.
    - Items in sets are **mutually exclusive**.
    - The probability of set elements is **relative** to the total.
    - If a range is defined for the set, items are drawn independently from the set as many times as the random number drawn from the range.
    - Each item in a set can have its own stock.
    - The set can have its own stock. If the set's stock drops to 0, the set is removed even if items in the set still have stock.
    - Each item in the set is distinguished from other standalone items in the loot table by a '-' at the beginning.
    ~~-----------------------------------------------------------------~~
    Practical example:
    ```
    gold: 40, 5-20
    legendary sword: 5, 1, stock:1, secret

    knight_armor: 20, 1-5, stock:5
    - breastplate: 5, 1, stock:4
    - greaves: 5, 1-2
    - gauntlet: 5, 1-2 stock:6
    - grimoire: 1, 1, secret
    - signet ring: 1, 1, stock:1, secret
    ```
    Notes:
    - The set's 1-5 range indicates that up to 5 items from the set can be drawn independently.
    - The set's stock (5) is lower than the total stock of the set (4+6+1 = 11). This implies that the set will probably be destroyed before the items' stocks are depleted.
    - The sum of weights is 5+5+5+1+1 = 17. This means that the `breastplate`, `greaves`, and `gauntlet` have a 5/17 chance of being drawn, while the `grimoire` and `signet ring` have a 1/17 chance of being drawn.
    - If the signet ring is drawn, its stock will reach 0. So the sum of weights becomes 16, which changes the probabilities: 5/16 for the `breastplate`, `greaves`, and `gauntlet`, and 1/16 for the `grimoire`.

loot_table__server_not_found = Server not found
    .title = Server not found
    .message = The server was not found.
                Please try again or contact support if the problem persists: {support}
loot_table__no_permission = Insufficient permissions
    .title = Insufficient permissions
    .message = You do not have permission to manage loot tables.
loot_table__target_not_found = Unknown target
    .title = Unknown target
    .message = Invalid target: must be a place category, a road channel, or a place sub-channel
loot_table__slash_only = Slash command
    .title = Slash command
    .message = This command can only be used as a slash command
loot_table__success = Loot table saved
    .title = Loot table saved
    .message = Loot table successfully saved
loot_table__invalid_min_max = Invalid range
    .title = Invalid range
    .message = Invalid quantity range: {$min} to {$max}. Min must be <= max.
loot_table__invalid_item_name = Invalid item name
    .title = Invalid item name
    .message = Invalid item or set name: {$name}
loot_table__modified = Loot Table Modified
    .title = Loot Table Modified
    .message = The loot table has successfully been modified.
loot_table__not_in_guild = Not in a Guild
    .title = Not in a Guild
    .message = This command can only be used within a server.
loot_table__universe_not_found = Universe Not Found
    .title = Universe Not Found
    .message = The universe for this server could not be found.
loot_table__character_not_found = Character Not Found
    .title = Character Not Found
    .message = You don't have a character in this universe. Please create one first.
loot_table__error_fetching_universe = Error Fetching Universe
    .title = Database Error
    .message = An error occurred while fetching the universe: {$error}
loot_table__error_fetching_character = Error Fetching Character
    .title = Database Error
    .message = An error occurred while fetching your character: {$error}
loot_table__error_fetching_channel_table = Error Fetching Channel Loot Table
    .title = Database Error
    .message = An error occurred while fetching the channel loot table: {$error}
loot_table__error_fetching_category_table = Error Fetching Category Loot Table
    .title = Database Error
    .message = An error occurred while fetching the category loot table: {$error}
loot_table__error_adding_inventory = Inventory Error
    .title = Database Error
    .message = An error occurred while adding items to your inventory: {$error}
loot_table__setup_channel = Setup Channel
    .title = Setup Channel
    .message = You cannot loot in a setup channel.
loot_table__no_loot_found = Nothing Found
    .title = Nothing Found
    .message = You searched thoroughly but found nothing this time.
loot_table__loot_success = Looting Successful
    .title = Looting Successful
    .message = You have found some items! {$items}
loot_table__deleted_log = The loot table for channel <#{$channel_id}> has been deleted because it is now empty.
loot_table__rate_limited = Cooldown
    .title = Cooldown
    .message = You must wait another {$error} seconds before you can search this area again.
loot_table__loot_started = Looting Started
    .title = Looting Started
    .message = You start searching the area. This will take {$error} seconds.
loot_table__loot_finished_title = Looting Finished
loot_table__loot_finished_message = Your looting in universe **{$universe}** with **{$character}** is finished! You found: {$items}
loot_table__loot_finished_late_message = Your looting in universe **{$universe}** with **{$character}** finished successfully (a bit late due to technical reasons)! You found: {$items}
loot_table__already_moving = Traveling
    .title = Action Impossible
    .message = You cannot loot while traveling.
loot_table__already_crafting = Crafting
    .title = Action Impossible
    .message = You cannot loot while crafting something.
loot_table__already_looting = Looting already in progress
    .title = Action Impossible
    .message = You are already looting an area.
loot_table__not_in_loot = Not looting
    .title = Action Impossible
    .message = You are not currently looting.
loot_table__stopped = Looting Cancelled
    .title = Success
    .message = Your looting has been cancelled.
loot_table__item_line = - {$item_name}
loot_table__item_line_quantity = - {$item_name} (x{$quantity})
loot_table__item_not_found = - {$item_name} (x{$quantity}) (Non-existent item in database)
loot_table__item_db_error = - {$item_name} (x{$quantity}) (Database error: {$error})
create_item__invalid_name = Invalid item name
    .title = Incorrect name
    .message = The item name **{$name}** contains unauthorized characters. Only alphanumeric characters, spaces, hyphens and underscores are allowed.
create_item__already_exists = An item with this name already exists in this universe.
    .title = Creation impossible
    .message = An item with this name already exists in this universe.
#Stats
stat_insert__failed = Failed to insert statistics
    .title = Failed to add stat
    .description = The stat could not be added.
resolve_stat__character_not_found = Character not found during stat resolution
    .title = Statistics error
    .message = Unable to find the character to calculate their statistics.
resolve_stat__database_error = Database error during stat resolution
    .title = Statistics error
    .message = A database error occurred while retrieving statistics.
#Reply
reply__reply_success = Success
    .title = Success
    .message = The operation was successful.
reply__reply_failed = Failed to send reply
    .title = Reply failed
    .description = The reply failed
#Universe
universe = universe
    .description = Universe management commands.
universe_create_universe = new_universe
    .description = Allows you to create a new universe. A server can only be attached to one universe at a time.
    .universe_name = name
    .universe_name-description = Name of the new Universe
    .setup_type = setup_type
    .setup_type-description = Configuration type for this server
universe_add_server = add
    .description = Adds this server to an existing universe.
    .setup_type = setup_type
    .setup_type-description = Configuration type for this server
universe_setup = setup
    .description = Configure or reconfigure the current server for the universe it is linked to.
    .setup_type = setup_type
    .setup_type-description = Type of setup to perform (Full or Partial).
universe_time = time
    .description = Displays the current time of the universe.

#Roads
road = road
    .description = Road management commands.
road_create_road = create_road
    .description = Creates a new road between two places.
    .place_one = place_one
    .place_one-description = First end of the road.
    .place_two = place_two
    .place_two-description = Second end of the road.
    .distance = distance
    .distance-description = Distance between the two places in kilometers.
    .secret_channel = secret
    .secret_channel-description = If true, the road will not be displayed on public maps.

#Places
place = place
    .description = Place management commands.
place_create_place = new_place
    .description = Creates a new category corresponding to a city or interaction place.
    .name = name
    .name-description = Name of the place to create.
create_place__new_place_title = Place: {$place_name}
create_place__channel_id = Place Id

#Characters
character = character
    .description = Character management commands.
character_create_character = new_character
    .description = Allows you to create your character in the universe. Only one character per player.
character_inventory = inventory
    .description = Allows to player to consult character's inventory.
#Travels
travel = travel
    .description = Allows you to move from one place to another.
travel_start = start
    .description = Starts a journey toward a destination.
    .destination = destination
    .destination-description = The place where you want to go (ID or mention).
travel_stop = stop
    .description = Stops your current journey on the road you are currently on.

#Misc
ping = ping
    .description = Measures the bot's latency.
support_command = support
    .description = Displays information to support the project.
start = start
    .description = Displays startup instructions.

#Server
id__nothing_to_delete = Nothing to delete
    .title = Nothing to delete
    .message = No item to delete was found
id__role_delete_success = Role successfully deleted
    .title = Deletion successful
    .message = The role has been successfully deleted
            Please try again or contact support if the problem persists: {support}
id__role_delete_failed = Failed to delete role
    .title = Deletion error
    .message = Unable to delete the role
            Please try again or contact support if the problem persists: {support}
id__channel_delete_sucess = Channel successfully deleted
    .title = Deletion successful
    .message = The channel has been successfully deleted
            Please try again or contact support if the problem persists: {support}
id__channel_delete_failed = Failed to delete channel
    .title = Deletion error
    .message = Unable to delete the channel
            Please try again or contact support if the problem persists: {support}

#Setup
SetupType = SetupType
FullSetup = Full
PartialSetup = Partial
cancel_setup = Cancel
continue_setup = Continue 
setup__continue_setup_message = Continue setup?
    .title = Continue setup
    .message = Do you want to continue the setup despite a previous setup? Missing channels and roles will be created.
setup__server_already_setup_timeout = Setup timeout exceeded
    .title = Timeout exceeded
    .message = The time to continue the setup has expired
partial_setup__get_guild_roles_error = Failed to retrieve guild roles
    .title = Setup error
    .message = Unable to retrieve roles from the server.
            Please try again or contact support if the problem persists: {support}
setup__server_not_found = Server not found
    .title = Server not found
    .message = This server is not registered in our database.
            Please try again or contact support if the problem persists: {support}
setup_server__cancelled = Setup cancelled
    .title = Setup cancelled
    .message = Server setup has been cancelled
setup_server__success = Setup successful
    .title = Success
    .message = The server has been successfully configured
setup_server__failed = Setup failed
    .title = Error
    .message = Server setup failed
            Please try again or contact support if the problem persists: {support}
setup__full_setup_success = Full setup successful
    .title = Setup completed
    .message = Full server setup has been successfully completed
            Please try again or contact support if the problem persists: {support}
admin_category_name = Administration
    .title = Administration
    .message = Administration category
            Please try again or contact support if the problem persists: {support}
setup__admin_category_not_created = Administration category not created
    .title = Creation error
    .message = Unable to create the administration category
            Please try again or contact support if the problem persists: {support}
nrp_category_name = Out of RP
setup__nrp_category_not_created = Out of RP category not created
    .title = Creation error
    .message = Unable to create the Out of RP category
            Please try again or contact support if the problem persists: {support}
rp_category_name = RP
setup__rp_category_not_created = RP category not created
    .title = Creation error
    .message = Unable to create the RP category
            Please try again or contact support if the problem persists: {support}
setup__roles_setup_failed = Role setup failed
    .title = Setup error
    .message = Role setup failed
            Please try again or contact support if the problem persists: {support}
log_channel_name = Logs
setup__log_channel_not_created = Logs channel not created
    .title = Creation error
    .message = Unable to create the log channel
            Please try again or contact support if the problem persists: {support}
commands_channel_name = Commands
setup__commands_channel_not_created = Commands channel not created
    .title = Creation error
    .message = Unable to create the commands channel
            Please try again or contact support if the problem persists: {support}
moderation_channel_name = Moderation
setup__moderation_channel_not_created = Moderation channel not created
    .title = Creation error
    .message = Unable to create the moderation channel
            Please try again or contact support if the problem persists: {support}
nrp_general_channel_name = General
setup__nrp_general_channel_not_created = Out of RP general channel not created
    .title = Creation error
    .message = Unable to create the Out of RP general channel
            Please try again or contact support if the problem persists: {support}
rp_character_channel_name = Character sheets
setup__rp_character_channel_not_created = Character sheets channel not created
    .title = Creation error
    .message = Unable to create the character sheets channel
            Please try again or contact support if the problem persists: {support}
universal_time_channel_name = Universal time
setup__universal_time_channel_not_created = Universal time channel not created
    .title = Creation error
    .message = Unable to create the universal time channel
            Please try again or contact support if the problem persists: {support}
rp_wiki_channel_name = Wiki

setup__wiki_channel_not_created = Wiki channel not created
    .title = Creation error
    .message = Unable to create the wiki channel
            Please try again or contact support if the problem persists: {support}
setup__rollback_failed = Failed to rollback changes
    .title = Rollback error
    .message = Unable to rollback the changes made
            Please try again or contact support if the problem persists: {support}
setup__channel_setup_failed = Channel setup failed
    .title = Setup error
    .message = Channel setup failed
            Please try again or contact support if the problem persists: {support}
guild_only = Command reserved for servers.
admin_role_name = Administrator
setup__admin_role_not_created = Administrator role not created
    .title = Creation error
    .message = Unable to create the Administrator role
            Please try again or contact support if the problem persists: {support}
moderator_role_name = Moderator
setup__moderator_role_not_created = Moderator role not created
    .title = Creation error
    .message = Unable to create the Moderator role
            Please try again or contact support if the problem persists: {support}
spectator_role_name = Spectator
setup__spectator_role_not_created = Spectator role not created
    .title = Creation error
    .message = Unable to create the Spectator role
            Please try again or contact support if the problem persists: {support}
player_role_name = Player
setup__player_role_not_created = Player role not created
    .title = Creation error
    .message = Unable to create the Player role
            Please try again or contact support if the problem persists: {support}
setup__error_during_role_creation = Error during role creation
    .title = Creation error
    .message = An error occurred during role creation
            Please try again or contact support if the problem persists: {support}
setup__reorder_went_wrong = Error during reordering
    .title = Reordering error
    .message = An error occurred during role reordering
            Please try again or contact support if the problem persists: {support}
road_channel_name = Roads
setup__road_category_not_created = Roads category not created
    .title = Creation error
    .message = Unable to create the Roads category
            Please try again or contact support if the problem persists: {support}
setup__server_update_failed = Failed to update server
    .title = Update error
    .message = Unable to update server information
            Please try again or contact support if the problem persists: {support}
setup__setup_success_message = Setup completed successfully
    .title = Success
    .message = The setup has been completed successfully

#create place
create_placce = new_place
create_place__server_not_found = Unknown server
    .title = Unknown server
    .message = The server does not appear to be registered. Run /{$universe} {$add_server} [setup type]
create_place__database_not_found = Database not found
    .title = Connection failed
    .message = The database connection failed.
            Please try again or contact support if the problem persists: {support}
create_place__role_not_created = Role creation failed
    .title = Role creation failed
    .message = The place role could not be created correctly.
            Please try again or contact support if the problem persists: {support}
create_place__rollback_complete = Rollback completed
    .title = Rollback performed
    .message = Something went wrong during place creation. A rollback has been performed.
create_role__rollback_failed = Rollback failed
    .title = Rollback failed
    .message = Something went wrong during place creation and the rollback failed.
            Please contact support: {support}
create_place__success = Place created
    .title = Place created
    .message = The place has been successfully created.

#Create road
create_road = create_road
create_road__server_not_found = Server not found
    .title = Server not found
    .message = The server does not appear to be registered. Run /{$universe} {$add_server} [setup type]
create_road__database_error = Database error
    .title = Database error
    .message = An error occurred while accessing the database.
                        Please try again or contact support if the problem persists: {support}
create_place__place_one_not_found = First place not found
    .title = First place not found
    .message = The first specified place was not found in the universe.
                        Please try again or contact support if the problem persists: {support}
create_place__place_two_not_found = Second place not found
    .title = Second place not found
    .message = The second specified place was not found in the universe.
                        Please try again or contact support if the problem persists: {support}
create_road__role_creation_failed = Role creation error
    .title = Role creation error
    .message = The road role could not be created correctly.
                        Please try again or contact support if the problem persists: {support}
create_road__create_channel_failed_rollback_success = Channel creation error
    .title = Channel creation error
    .message = The channel could not be created, but the changes have been rolled back.
                        Please try again or contact support if the problem persists: {support}
create_road__create_channel_failed_rollback_failed = Critical error
    .title = Critical error
    .message = Channel creation failed and the rollback could not be performed.
                        Please contact support: {support}
create_road__insert_road_failed_rollback_success = Insertion error
    .title = Insertion error
    .message = The road could not be saved, but the changes have been rolled back.
                        Please try again or contact support if the problem persists: {support}
create_road__insert_road_failed_rollback_channel_failed = Critical error
    .title = Critical error
    .message = The road registration failed and the channel rollback failed.
                        Please contact support: {support}
create_road__insert_road_failed_rollback_role_failed = Critical error
    .title = Critical error
    .message = The road registration failed and the role rollback failed.
                        Please contact support: {support}
create_road__success = Road created
    .title = Road created
    .message = The road has been successfully created
create_road__limit_reached = Road limit reached
    .title = Limit reached
    .message = One of the places has already reached the maximum of 25 roads (excluding secret roads).
create_road__already_exists = Road already exists
    .title = Existing road
    .message = A road already exists between these two places.
create_road__universe_mismatch = Different universe
    .title = Different universe
    .message = Both places must belong to the same universe.
create_road__invalid_place_one = Invalid first place ID
    .title = Invalid first place
    .message = The ID or mention of the first place is invalid. Use an ID or a mention <#id>.
create_road__invalid_place_two = Invalid second place ID
    .title = Invalid second place
    .message = The ID or mention of the second place is invalid. Use an ID or a mention <#id>.

#Create character
create_character = new_character
character_modal_title = Create new character
create_character__delete_character = Cancel
create_character__submit_character = Submit
create_character__modify_character = Modify
create_character__refuse_character = Refuse
create_character__accept_character = Accept
character_special_request = Special request
character_story = Character's story
character_description = Physical description
character_name = Character's name
create_character__start_place = Starting place
create_character__submit_notification = @here A character sheet is awaiting verification:

character_reject_reason = Reject reason

create_character__no_universe_found = Universe not found
    .title = Universe not found
    .message = There is no existing universe for this server.
create_character__database_error = Database error
    .title = Database error
    .message = Unable to access the database.
            Please try again or contact support if the problem persists: {support}
create_character__wrong_channel = Wrong channel
    .title = Wrong channel
    .message = This command must be used in the character sheet channel.
create_character__character_already_existing = Character already exists
    .title = Character already exists
    .message = You already have a character. You can't create another one.
CharacterModal = character_modal
    .character_name = Name
    .character_description = Character's description
    .placeholder = Describe your character here...
    .character_story = Character's story
    .value = Once upon a time...
    .character_special_request = Special requests
create_character__submitted = Character sent
    .title = Character sent
    .message = Your character sheet has been sent for verification. Please wait for a moderator's decision.
create_place__character_too_long = Character sheet too long
    .title = Character sheet too long
    .message = The character sheet is too long to be displayed. Please try again.
character_instruction = Fill following fields to describe your character.
    ► All paragraph fields are limited to 1024 characters.
    ► A 30 minutes timeout is set for security.
    You can click on the modify button to change your draft before submitting it to moderators.
create_character__timed_out = Timed out
    .title = Timed out
    .message = The character creation process timed out.
create_character__guild_only = Guild only
    .title = Guild only
    .message = This command can only be used within a server.
create_character__delete_successfull = Canceled
    .title = Character creation canceled
    .message = Your character creation process has been successfully canceled.
delete_character = Character deleted
    .title = Character deleted
    .message = The character sheet has been successfully deleted.
create_character__not_owner = Not owner
    .title = Not owner
    .message = You are not the owner of this character. You cannot perform this action.
create_character__no_member = Member not found
    .title = Error
    .message = Unable to find member information.
create_character__no_permission = Permission denied
    .title = Permission denied
    .message = You do not have the required permissions (Moderator or Administrator) to perform this action.
create_character__invalid_footer = Invalid interaction
    .title = Error
    .message = The interaction metadata is invalid.
create_character__invalid_embed_title = Invalid embed title
    .title = Error
    .message = The character sheet title is invalid.
create_character__message_not_found = Message not found
    .title = Error
    .message = The character sheet message could not be found.
create_character__refused = Character refused
    .title = Character refused
    .message = The character has been refused by a moderator.
accept_character = Character accepted
    .title = Character accepted
    .message = The character has been successfully accepted and added to the universe.
create_character__type_mismatch = Type mismatch
    .title = Validation error
    .message = One of the stat values provided does not match the expected type.
create_character__invalid_place_selected = Invalid place selected. Please choose a valid category for the character.
create_character__invalid_interaction = Invalid interaction data.
create_character__choose_place = Choose a place
    .title = Choose a place
    .message = Please select the category where the character will be located.
character_stat_input = Character's statistics
accept_character__no_player_role_id = Server not configured
    .title = Server not configured
    .message = The {player_role_name} role hasn't been found.
accept_character__member_not_found = Member not found during acceptance.
    .title = Acceptance error
    .message = Unable to find the user on the server.

travel__server_not_found = Server not found
    .title = Server not found
    .message = The server does not appear to be registered.
travel__place_not_found = Place not found
    .title = Place not found
    .message = The specified destination does not exist in this universe.
travel__character_not_found = Character not found
    .title = Character not found
    .message = You do not have a character in this universe.
travel__database_error = Database error
    .title = Database error
    .message = An error occurred while accessing the database.
travel_without_destination__database_error = Database error
    .title = Database error
    .message = Unable to retrieve available roads.
travel_without_destination__reply_failed = Sending error
    .title = Sending error
    .message = Unable to display the destination selection menu.
travel__source_place_not_found = Source place not found
    .title = Source place not found
    .message = Your current position is not recognized as a valid place.
travel__started = Journey started
travel__stopped = Journey stopped. You can now choose a destination or stay here.
travel__not_in_move = You are not currently traveling.
    .title = Journey started
    .message = You have started your journey to {$destination}.
travel__already_moving_to_destination = Already on the way
    .title = Already on the way
    .message = You are already moving toward this destination.
travel__invalid_road_destination = Invalid destination
    .title = Invalid destination
    .message = You cannot go to this place from your current position on the road.
move_from_place__road_not_found = No road found
    .title = No road found
    .message = There is no direct road between your current position and {$destination}.
travel__no_road_available = No available roads
    .title = No available road
    .message = No available road seems to be accessible from here. Maybe a secret road exists ?

travel__moving_to_place = `{$user} is moving toward {$destination}.`
travel__reached_destination = `{$user} has reached {$destination}.`
travel__arrived_at_destination = `{$user} has just arrived.`
travel__taking_unknown_road = `{$user} is taking an unknown road.`
travel__invitation = Border reached
    .title = Border reached
    .message = **_{$user}, you have reached the border of a region in the universe {$universe}! Here is the invitation to continue your journey: {$link} _**

#Recipes
recipe = recipe
    .description = Manage crafting recipes
recipe_create = create
    .description = Create a new recipe
    .name = name
    .name-description = Recipe name
    .delay = delay
    .delay-description = Crafting delay in seconds
recipe_craft = craft
    .description = Craft an item from a recipe
    .recipe_name = recipe_name
    .recipe_name-description = Name of the recipe to use

recipe__modal_title = Recipe Editor
recipe__modal_field_name = Recipe Content
recipe__modal_placeholder = # Recipe Syntax Guide
    - `> [item name] [quantity]` : Item obtained (result).
    - `< [item name] [quantity]` : Item used (ingredient).
    - `- [item name]` : Tool needed (not consumed).
    
    Example:
    ```
    - Anvil
    - Hammer
    < Iron Ingot 2
    < Coal 1
    > Iron Sword 1
    ```

recipe__server_not_found = Server not found
    .title = Server not found
    .message = The server was not found.
recipe__character_not_found = Character not found
    .title = Character not found
    .message = You don't have a character in this universe.
recipe__item_not_found = Item not found
    .title = Item not found
    .message = The item "{$name}" does not exist in this universe's database.
recipe__not_found = Recipe not found
    .title = Recipe not found
    .message = No recipe named "{$recipe_name}" was found.
recipe__missing_tool = Missing tool
    .title = Missing tool
    .message = You are missing one or more tools required for this recipe.
recipe__missing_ingredient = Missing ingredient
    .title = Missing ingredient
    .message = You do not have all the necessary ingredients in your inventory.
recipe__create_success = Recipe created
    .title = Success
    .message = The recipe has been successfully saved.
recipe__no_permission = You do not have permission to create a recipe.
    .title = Error
    .message = You must be an administrator or have the player role to create a recipe.
recipe__submit_success = Submission successful
    .title = Success
    .message = Your recipe has been submitted for approval.
recipe__validation_title = Recipe Validation
recipe__delay_field = Delay (sec)
recipe__creator_field = Creator
recipe__into_wiki_field = Wiki
recipe__submit_notification = @here A new recipe sheet is awaiting verification:
recipe__approve = Approve
recipe__reject = Refuse
recipe__modify = Modify
recipe__approved = Recipe approved
    .title = Approved
    .message = The recipe has been approved and saved.
recipe__rejected = Recipe rejected
    .title = Rejected
    .message = The recipe has been rejected.
recipe__modified = Recipe modified
    .title = Modified
    .message = The recipe has been successfully modified.
recipe__invalid_embed = Invalid embed
    .title = Error
    .message = The recipe embed is invalid or corrupted.
recipe__no_embed = Missing embed
    .title = Error
    .message = Unable to find the recipe embed.
recipe__no_content = Missing content
    .title = Error
    .message = The recipe contains no text.
recipe__guild_only = Server required
    .title = Error
    .message = This action must be performed in a Discord server.
recipe__craft_success = Crafting successful
    .title = Crafting successful: {$recipe_name}
    .message = You have successfully crafted the item!
recipe__craft_started = Crafting started
    .title = Crafting {$recipe_name}
    .message = Crafting has started. Remaining time: **{$delay}** seconds.
recipe__craft_finished_title = Crafting finished
recipe__craft_finished_message = You finished crafting **{$recipe_name}** in universe **{$universe}**.
recipe__craft_finished_late_message = Your crafting of **{$recipe_name}** in the universe **{$universe}** is finished! It was delayed for technical reasons, we apologize for the inconvenience.
recipe__craft_stopped = Crafting cancelled
    .title = Cancellation successful
    .message = Current crafting has been cancelled. (Ingredients already consumed are not returned)
recipe__no_craft_in_progress = No craft in progress
    .title = Error
    .message = You don't have any crafting in progress.
recipe__cannot_craft_while_moving = Currently moving
    .title = Error
    .message = You cannot craft while you are moving.
recipe__craft_already_in_progress = Craft already in progress
    .title = Error
    .message = You already have a crafting process in progress.
travel__cannot_move_while_crafting = Crafting in progress
    .title = Error
    .message = You cannot move while crafting. Use `/recipe stop` to cancel the craft.
recipe__empty_recipe = Empty recipe
    .title = Error
    .message = The recipe must contain at least one ingredient or one result.
recipe__invalid_line = Invalid line
    .title = Syntax error
    .message = A line in the recipe is malformed.
recipe__error_during_consumption = Consumption error
    .title = Critical error
    .message = An error occurred during ingredient consumption.
recipe__recipe_instructions = # Recipe Syntax Guide
    - `> [item name] [quantity]` : Item obtained (result).
    - `< [item name] [quantity]` : Item used (ingredient).
    - `- [item name]` : Tool needed (not consumed).
    - Quantity defaults to 1 if not specified.
    - Lines starting with `#` are comments.
    - __Note__: You must have the tools in your inventory or they must be placed in the channel.

# Universal Time
time = time
universe_time__current_time = Universe Time
    .title = Universal Time
    .message = It is currently **{$time}** in this universe.
            Current phase: **{$phase}**
universe_time__invalid_modifier = Invalid time modifier
    .title = Time Error
    .message = The universe time modifier is invalid.
time__midnight = **_It is midnight. Silence falls upon the universe._**
time__sunrise = **_The sun rises, a new day begins._**
time__noon = **_It is noon. The sun is at its zenith._**
time__sunset = **_The sun sets, the shadows grow longer._**

#Create Item
item = item
    .description = Item related commands.
item_lookup = lookup
    .description = Shows details of an item you own via its inventory ID
    .id = id
    .id-description = The inventory line ID (received by DM)
item-create = create
    .description = Create a new item
    .name = name
    .name-description = Item name
    .usage = usage
    .usage-description = Item usage type
    .into_wiki = into_wiki
    .into_wiki-description = Whether to add the item to the wiki
    .inventory_size = inventory_size
    .inventory_size-description = Item inventory size (0 for none)
    .image = image
    .image-description = Item image
    .description = description
    .description-description = Item description
    .secret_informations = secret_informations
    .secret_informations-description = Secret information only visible to owners

item_place = place
    .description = Place an object in the current channel.
    .inventory_id = inventory_id
    .inventory_id-description = ID of the inventory entry of the item to place
    .immutable = immutable
    .immutable-description = Is the object immutable? (Admin only)

item_usage_title = Usage type
item_inventory_size = Inventory size
item_lookup_usage = Usage
item_lookup_secret = Secret Information
item_lookup_effects = Effects
item_lookup_stat = Stat
item_lookup_value = Value
item_lookup_type = Type
item_no_description = _No description_
item_placed_success = Object placed!
    .title = Object placed!
    .message = You have placed **{$item_name}** in **#{$channel_name}**.
item_placed_rp = _**{$item_name}** was placed by **{$character_name}**._
item_immutable_footer = This object is immutable.

ItemUsage = ItemUsage
Consumable = Consumable
Usable = Usable
Wearable = Wearable
Placeable = Placeable
None = Other
inventory__empty = Empty Inventory
    .title = Empty Inventory
    .message = You don't have any items in your inventory.
inventory__empty_description = _The inventory is empty._
inventory__refresh_button = Refresh
inventory__lookup_hint = Use `/item lookup [id]` for more details.
inventory__title = {$character_name}'s Inventory
inventory__universe_field = Universe
inventory__previous_button = Previous
inventory__next_button = Next
inventory__page_footer = Page {$current} of {$total} - Use `/item lookup [ID]` to see item details
inventory__not_in_guild = Server only
    .title = Server only
    .message = This command must be used in a server.
item__not_found = Item Not Found
    .title = Item Not Found
    .message = No item with that name was found in this universe.
item__not_found_in_inventory = Item not found in inventory
    .title = Item not found in inventory
    .message = This ID does not correspond to any item you currently own.
item__not_your_item = Not your item
    .title = Not your item
    .message = This item does not belong to you.
item__invalid_id = Invalid ID
    .title = Invalid ID
    .message = The provided inventory ID is invalid.
item__no_search_criteria = Missing criteria
    .title = Missing criteria
    .message = Please provide either a name or an ID for the search.
item__server_not_found = Server not found
    .title = Server not found
    .message = The server was not found.
item__not_placeable = Item not placeable
    .title = Item not placeable
    .message = This item cannot be placed.
item__no_permission = Insufficient permissions
    .title = Insufficient permissions
    .message = You do not have the required permissions (administrator or player role) to create an item.
item__no_usage = Item usage not found
    .title = Missing data
    .message = The item usage could not be determined from the validation message.
item__no_embed = Embed not found
    .title = Missing data
    .message = Unable to find the validation embed.
item__guild_only = Server only
    .title = Server only
    .message = This action can only be performed on a server.
item__invalid_embed = Invalid embed
    .title = Data error
    .message = The validation embed structure is incorrect.
item__not_in_guild_channel = Not a guild channel
    .title = Channel error
    .message = This command must be used within a guild channel.
item__not_in_category = No category
    .title = Channel error
    .message = This channel is not in a category.
item__not_a_place = Not a place
    .title = Place not recognized
    .message = This channel is not associated with a valid Place.
item__failed_to_remove = Removal failed
    .title = Inventory error
    .message = Unable to remove the item from your inventory.
create_item__db_error = Database Error
    .title = Creation Error
    .message = An error occurred while creating the item in the database.
create_item__validation_title = New Item Validation
create_item__creator_field = Creator
create_item__into_wiki_field = Include in Wiki
create_item__secret_field = Secret Information
create_item__submit_success = Item Submitted
    .title = Submission successful
    .message = Your item has been sent to administrators for validation.
create_item__approved = Item Approved
    .title = Approval successful
    .message = The item has been created and added to the database (and wiki if requested).
create_item__rejected = Item Rejected
    .title = Rejection completed
    .message = Item creation has been refused.

item__approve = Approve
item__reject = Reject
item_delete = delete
    .description = Deletes an item from the universe (Admin only).
    .name = name
    .name-description = Name of the item to delete.

item_delete__not_in_guild = Server only
    .title = Server only
    .message = This command must be used in a server.
item_delete__server_not_found = Server not found
    .title = Server not found
    .message = The server was not found.
item_delete__not_found = Item not found
    .title = Item not found
    .message = No item named **{$name}** was found in this universe.
item_delete__notification_title = Item Deleted
item_delete__notification = The item **{$item_name}** has been deleted from the universe **{$universe_name}** by an administrator. It has been removed from **{$character_name}**'s inventory.
item_delete__success = Item deleted
    .title = Deletion successful
    .message = The item **{$name}** has been successfully deleted from the universe, inventories, and wiki.\n{$affected_recipes}
item_delete__affected_recipes = Here is affected recipes by the item deletion :{$affected_recipes}

use__universe_not_found = Universe Not Found
    .title = Universe Not Found
    .message = The universe associated with this server could not be located.
use__character_not_found = Character Not Found
    .title = Character Not Found
    .message = You must have a character created to use items.
use__invalid_tool_id = Invalid Tool ID
    .title = Invalid ID
    .message = The provided tool ID is not a valid MongoDB identifier.
use__no_tools_found = No Tools Found
    .title = No Tools
    .message = No usable objects were found in this channel.
use__list_tools = Available Objects
    .title = Available Objects
    .description = Here are the objects you can interact with in this location:
    .footer = Page {$current} of {$total} • Use /item lookup [ID] for more details
    .title = Usable objects in this channel
    .message = Here are the objects you can interact with:
        {$tools}
use__tool_not_found = Tool Not Found
    .title = Tool Not Found
    .message = The specified tool cannot be found or no longer exists.
use__no_inventory = No Inventory
    .title = Cannot Use
    .message = This object does not have any storage space.
use__only_slash_command = Command Error
    .title = Error
    .message = This interaction can only be initiated via a slash command.
use__empty_inventory = The inventory is empty.
use__modal_character_inventory_label = Your Inventory
use__modal_label = Transfer Actions
use__modal_chest_inventory_label = Chest Content
use__modal_instructions_label = Syntax Guide
use__modal_instructions_value = # Transaction Syntax Guide
    - `> [item_name] [quantity]` : Take an ITEM from the tool.
    - `< [item_name] [quantity]` : Deposit an ITEM into the tool.
    - Quantity defaults to 1 if not specified.
    - __Note__: Final item count must not exceed chest capacity.
use__transfer_success = Transfer Successful
    .title = Transfer Completed
    .message = Items have been successfully transferred.
