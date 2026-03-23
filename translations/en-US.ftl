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
    - The probability of each line is **absolute** (except in sets) and given in % implicitly (no need to specify %): `40`
    - Each line of the loot table has a chance to be drawn independently of the others. The sum of probabilities can therefore exceed 100%.
    - The `min-max` range can be replaced by a single number if min and max are identical: `2-2` becomes `2`
    - __Optional__: If the loot is in limited quantity, the stock parameter can be indicated: `stock:10`
    - __Optional__: The `secret` keyword can be used so that the item does not appear in loot tables in the wiki.
    - If a stock drops to 0, items are removed from the table and a message is sent to the logs channel.
    ## Items
    - Item names are used to identify objects and are case-sensitive.
    Example template for an item:
    ```
    ­[item name]:[probability in %], [min-max range], stock:[number], secret
    ```
    ## Sets
    - Contain a list of items.
    - Set names are purely technical and will not be displayed in the wiki.
    - Set elements are **mutually exclusive**.
    - The probability of set elements is **relative** to the total.
    - Set elements have the same syntax as objects.
    - If a range is defined for the set, items are drawn independently as many times as the random number drawn in the range.
    - A stock can be defined for each item independently of the set. If the set's stock drops to 0, it is deleted, even if there were items in stock in that set.
    - Each set element is distinguished from other loot table objects by a '-' at the beginning.
    Example template for a set:
    ```
    ­[set name]:[probability in %], [min-max], stock:[number], secret
    - [item name]:[probability in %], [min-max], stock:[number], secret
    - ...
    ```
    Practical example:
    ```
    gold: 40, 5-20
    legendary_sword: 5, 1, stock:1, secret

    knight_armor: 20, 1-5, stock:5
    - breastplate: 5, 1,  stock:4
    - greaves: 5, 1-2
    - gauntlets: 5, 1-2 stock:6
    - grimoire: 1, 1, secret
    - signet_ring: 1, 1, stock:1, secret
    ```
    Notes:
    - The set range 1-5 indicates that up to 5 elements of the set can be drawn independently.
    - For each draw in the set, it is possible to draw multiple items according to the ranges.
    - The set stock (5) is lower than the total set stock (4+6+1 = 11). This implies that the set will probably be destroyed before the item stocks are depleted.
    - The sum of relative probabilities is 5+5+5+1+1 = 17. This means that breastplate, greaves and gauntlets have a 5/17 chance of being drawn, while grimoire and signet_ring have a 1/17 chance of being drawn.


loot_table__server_not_found = Server not found
loot_table__no_permission = You do not have permission to manage loot tables
loot_table__target_not_found = Invalid target: must be a place category, a road channel, or a place sub-channel
loot_table__slash_only = This command can only be used as a slash command
loot_table__success = Loot table saved successfully
loot_table__invalid_min_max = Invalid quantity range: {$min} to {$max}. Min must be <= max.
loot_table__invalid_item_name = Invalid item or set name: {$name}

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
    .description = Manage items
item-create = create
    .description = Create a new item
    .name = name
    .name-description = Item name
    .usage = usage
    .usage-description = Item usage type
    .into_wiki = into_wiki
    .into_wiki-description = Whether to add the item to the wiki
    .image = image
    .image-description = Item image
    .description = description
    .description-description = Item description
    .secret_informations = secret_informations
    .secret_informations-description = Secret information only visible to owners
item_usage_title = Usage type
ItemUsage = ItemUsage
Consumable = Consumable
Usable = Usable
Wearable = Wearable
Placeable = Placeable
None = Other