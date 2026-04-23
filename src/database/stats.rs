//!  function that resolves to:
//!  
//!  * `Ok(Option<Stat>)` - If the query succeeds, returns `Some(Stat)` if the record is found,
//!    or `None` if no matching record exists.
//!  * `Err(Error)` - Returns an error if the query process fails.
//! 
//!  # Errors
//! 
//!  This function will return an error in the following scenarios:
//!  - Unable to establish a connection to the database using `connect_db()`.
//!  - The query operation fails in the database client.
//! 
//!  # Examples
//! 
//!  ```rust
//!  let universe_id = "sample_universe_id";
//!  let stat_name = "Health";
//!  
//!  let stat = Stat::get_stat_by_name(universe_id, stat_name).await;
//!  match stat {
//!      Ok(Some(stat)) => println!("Stat retrieved successfully: {:?}", stat),
//!      Ok(None) => println!("Stat not found."),
//!      Err(e) => eprintln!("Failed to retrieve stat: {:?}", e),
//!  }
//!  ```
//! 
//!  # Notes
//! 
//!  - This function assumes that the `connect_db()` function is properly implemented and
//!    that the `DB_CLIENT` singleton is functional.
//!  - The query uses `name` as the matching parameter to retrieve a specific `Stat`. If 
//!    multiple documents with the same name exist, only one will be returned, as specified
//!    by the database driver's behavior.
//! 
//!  # Dependencies
//! 
//!  This function relies on the following:
//!  - A global `DB_CLIENT` to establish and manage database connections.
//!  - `STATS_COLLECTION_NAME`, which specifies the target collection.
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{CHARACTERS_COLLECTION_NAME, ROADS_COLLECTION_NAME, STATS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::{Modifier, ModifierType};
use crate::database::characters::Character;
use crate::database::road::Road;
use crate::discord::poise_structs::Error;

pub static SPEED_STAT: &str = "speed";

/// Represents a value that can hold different types of statistical data. 
///
/// This enum is used to encapsulate multiple types of data commonly encountered 
/// in statistical contexts, such as integers, floating-point numbers, strings, 
/// and boolean values. It derives several useful traits to enable serialization, 
/// comparison, cloning, and debugging.
///
/// # Variants
///
/// * `Int(u32)` - Represents an unsigned 32-bit integer value.
/// * `Float(f32)` - Represents a 32-bit floating-point value.
/// * `Text(String)` - Represents a string value.
/// * `Bool(bool)` - Represents a boolean value (`true` or `false`).
///
/// # Derives
///
/// * `Serialize` and `Deserialize` - Enables serialization and deserialization of the enum, 
///   making it compatible with data formats like JSON or MessagePack.
/// * `Debug` - Enables formatting the enum for debugging purposes.
/// * `Clone` - Allows creating deep copies of the enum.
/// * `PartialOrd` and `PartialEq` - Provides support for partial ordering 
///   and equality comparisons.
///
/// # Attributes
///
/// * `#[serde_as]` - Custom Serde attribute for advanced serialization/deserialization. 
///
/// This attribute may be used in conjunction with additional Serde annotations to 
/// customize how the enum and its variants are serialized/deserialized.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StatValue {
    pub I64(i64),
    pub F64(f64),
    pub String(String),
    pub Bool(bool)
}

impl StatValue {
    pub fn as_f64(&self) -> f64 {
        match self {
            StatValue::I64(v) => *v as f64,
            StatValue::F64(v) => *v,
            StatValue::String(s) => s.parse().unwrap_or(0.0),
            StatValue::Bool(b) => if *b { 1.0 } else { 0.0 },
        }
    }
}

impl PartialOrd for StatValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_f64().partial_cmp(&other.as_f64())
    }
}

/// Represents a `Stat` structure, which holds information about a specific statistical
/// property within a given universe or context. This struct is designed to be serialized
/// and deserialized for data interchange, and can be cloned for convenience.
///
/// # Attributes
///
/// * `_id` (`ObjectId`):
///   The unique identifier for this `Stat`. This field is serialized as `_id` in the resulting data format.
///
/// * `universe_id` (`ObjectId`): 
///   The identifier for the universe or scope to which this `Stat` belongs.
///
/// * `name` (`String`): 
///   The name of the `Stat`, typically used for display or identification purposes.
///
/// * `base_value` (`StatValue`): 
///   The initial or default value of the `Stat`.
///
/// * `formula` (`Option<String>`): 
///   An optional formula associated with the `Stat` for deriving its value dynamically.
///   If no formula is provided, the property may rely solely on its `base_value`.
///
/// * `min` (`Option<StatValue>`): 
///   An optional minimum value constraint for the `Stat`.
///
/// * `max` (`Option<StatValue>`): 
///   An optional maximum value constraint for the `Stat`.
///
/// * `modifiers` (`Vec<Modifier>`): 
///   A collection of `Modifier` objects that dynamically alter the `Stat` value.
///   Modifiers allow for flexible adjustments based on context or conditions.
///
/// # Derivable Traits
///
/// * `Serialize` and `Deserialize`: 
///   Enables serialization and deserialization of the `Stat` struct using libraries like Serde.
///
/// * `Debug`: 
///   Allows debugging through formatted output of the `Stat` structure.
///
/// * `Clone`: 
///   Enables duplication of a `Stat` instance.
///
/// # Serde Attributes
///
/// * `#[serde_as]`: 
///   Adds advanced serialization support through `serde_with` crate, if applicable.
///
/// * `#[serde(rename = "_id")]`: 
///   Renames the `_id` field in serialized output to explicitly match the desired naming convention.
///
/// # Usage Example
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use mongodb::bson::oid::ObjectId;
///
/// let stat = Stat {
///     _id: ObjectId::new(),
///     universe_id: ObjectId::new(),
///     name: String::from("Health"),
///     base_value: StatValue::Int(100),
///     formula: Some(String::from("base_value + modifier")),
///     min: Some(StatValue::Int(0)),
///     max: Some(StatValue::Int(200)),
///     modifiers: vec![],
/// };
/// ```
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Stat {
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,
    pub name: String,
    pub base_value: StatValue,
    pub formula: Option<String>,
    pub min: Option<StatValue>,
    pub max: Option<StatValue>,
    pub modifiers: Vec<Modifier>
}

impl Stat {
    /// Asynchronously inserts a `Stat` document into the database.
    ///
    /// This function establishes a database connection, retrieves the corresponding
    /// collection for the given universe ID, and then inserts the current instance 
    /// of `self` into the `STATS_COLLECTION_NAME` collection. 
    ///
    /// # Returns
    ///
    /// * `Ok(Stat)` - Returns a cloned instance of `self` on successful insertion.
    /// * `Err(Error)` - Returns an error if the insertion process fails.
    ///
    /// # Errors
    ///
    /// This function will return an error in the following scenarios:
    /// - Unable to establish a connection to the database using `connect_db()`.
    /// - The insertion operation fails with the database client, resulting in `"stat_insert__failed"`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let stat_instance = Stat {
    ///     // populate fields
    /// };
    ///
    /// let result = stat_instance.insert_stat().await;
    ///
    /// match result {
    ///     Ok(stat) => println!("Stat inserted successfully: {:?}", stat),
    ///     Err(e) => eprintln!("Failed to insert stat: {:?}", e),
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - Ensure the `connect_db()` function is properly defined and returns a valid 
    /// MongoDB database client.
    /// - The `universe_id` field in `self` is expected to be convertible to a string
    /// for use as the database name.
    /// - The `STATS_COLLECTION_NAME` should be defined elsewhere in the codebase as a 
    /// constant representing the name of the collection to use.
    ///
    /// # Dependencies
    ///
    /// This function uses the following external components:
    /// - A global `DB_CLIENT` which leverages `get_or_init` to initialize or retrieve
    ///   an existing connection.
    /// - `STATS_COLLECTION_NAME` which determines the collection into which the 
    ///   document is inserted.
    ///
    /// # Safety
    ///
    /// If `DB_CLIENT` initialization fails or the database operation fails, the proper 
    /// error handling mechanism should be in place to avoid runtime panics.
    pub async fn insert_stat(&self) -> Result<Stat, Error>{
        let db_client = get_db_client().await;
        let result = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Stat>(STATS_COLLECTION_NAME)
            .insert_one(self)
            .await;
        match result {
            Ok(_) => {
                Ok(self.clone()) }
            Err(_) => { Err("stat_insert__failed".into()) }
        }
    }

    /// Retrieves a specific statistic by its name from the database.
    ///
    /// # Parameters
    /// - `universe_id`: A string slice representing the ID of the database (or namespace) to query.
    /// - `name`: A string slice representing the name of the statistic to retrieve.
    ///
    /// # Returns
    /// An `async` function that returns a `mongodb::error::Result`:
    /// - `Ok(Some(Stat))`: If a statistic with the given name is found, it returns a wrapped `Stat` object.
    /// - `Ok(None)`: If no statistic with the given name is found.
    /// - `Err(mongodb::error::Error)`: If there is an error during the database query operation.
    ///
    /// # Behavior
    /// - Establishes a connection to the database using a cached `DB_CLIENT`.
    /// - Queries the specified collection (`STATS_COLLECTION_NAME`) within the database identified
    ///   by `universe_id` for the document where the "name" field matches the given `name`.
    ///
    /// # Example
    /// ```rust
    /// let stat = get_stat_by_name("game_universe", "player_kills").await?;
    /// match stat {
    ///     Some(stat) => println!("Found stat: {:?}", stat),
    ///     None => println!("No statistic found with the given name."),
    /// }
    /// # Ok::<(), mongodb::error::Error>(())
    /// ```
    ///
    /// # Dependencies
    /// - The function relies on a globally initialized `DB_CLIENT` to manage database connections.
    /// - Assumes `STATS_COLLECTION_NAME` is defined elsewhere in the code.
    /// - The `Stat` struct represents the schema of the statistic documents.
    ///
    /// # Errors
    /// This function may return a `mongodb::error::Error` if:
    /// - The database connection cannot be established.
    /// - The query execution fails.
    pub async fn get_stat_by_name(universe_id: &str, name: &str) -> mongodb::error::Result<Option<Stat>> {
        let db_client = get_db_client().await;
        let filter = doc! { "name": name, "universe_id": universe_id };
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Stat>(STATS_COLLECTION_NAME)
            .find_one(filter)
            .await
    }
    
    /// Checks if the `base_value` is within the optional `min` and `max` bounds.
    ///
    /// This method evaluates whether `base_value` respects the range defined by
    /// the `min` and `max` values, if they are present:
    /// - If `min` is defined, `base_value` must be greater than or equal to `min`.
    /// - If `max` is defined, `base_value` must be less than or equal to `max`.
    ///
    /// # Returns
    /// - `true` if `base_value` is within the specified bounds or if no bounds are set.
    /// - `false` if `base_value` is out of bounds.
    ///
    /// # Examples
    /// ```
    /// let instance = MyStruct {
    ///     base_value: 10,
    ///     min: Some(5),
    ///     max: Some(15),
    /// };
    /// assert_eq!(instance.is_within_bounds(), true);
    ///
    /// let instance = MyStruct {
    ///     base_value: 20,
    ///     min: Some(5),
    ///     max: Some(15),
    /// };
    /// assert_eq!(instance.is_within_bounds(), false);
    /// ```
    ///
    /// Note: This function assumes that `min` is less than or equal to `max` if both are defined.
    pub fn is_within_bounds(&self) -> bool {
        if let Some(min) = &self.min {
            if self.base_value < *min {
                return false;
            }
        }
        if let Some(max) = &self.max {
            if self.base_value > *max {
                return false;
            }
        }
        true
    }

    pub async fn resolve(self, category_id: u64, user_id: u64) -> Result<(StatValue, Option<Modifier>), Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);

        // 1. Recover stat in the universe (global)
        let mut universe_stat = db.collection::<Stat>(STATS_COLLECTION_NAME)
            .find_one(doc! { "name": &self.name, "universe_id":  self.universe_id })
            .await.unwrap_or_else(|_| None);

        // 2. Recover location stat/modifiers (Espace: Salon ou Route)
        let mut road = db.collection::<Road>(ROADS_COLLECTION_NAME)
            .find_one(doc! { "$or": [
                { "channel_id": category_id.to_string(), "universe_id":  self.universe_id},
                { "place_one_id": category_id.to_string(), "universe_id":  self.universe_id },
                { "place_two_id": category_id.to_string(), "universe_id":  self.universe_id }
            ] })
            .await.unwrap_or_else(|_| None);

        // 3. Recover area stat/modifiers (Lieu: Catégorie ou Route)
        // Note: Ici, category_id semble être utilisé pour l'espace.
        // On récupère le Place correspondant pour avoir la catégorie parente si nécessaire,
        // ou on considère que si on est dans un salon, le Place est le "Lieu".
        let mut place = crate::database::places::get_place_by_category_id(self.universe_id, category_id)
            .await.unwrap_or_else(|_| None);

        // 4. Recover player stat/modifiers
        let mut character = match db.collection::<Character>(CHARACTERS_COLLECTION_NAME)
            .find_one(doc! { "user_id": user_id.to_string(), "universe_id":  self.universe_id })
            .await {
                Ok(res) => res,
                Err(_) => return Err("resolve_stat__database_error".into())
            };

        let stat_id = self._id;



        // Cleanup expired modifiers
        if let Some(ref mut _character) = character {
            let mut changed = false;
            for stat in &mut _character.stats {
                let initial_len = stat.modifiers.len();
                stat.modifiers.retain(|m| m.is_active());
                if stat.modifiers.len() != initial_len {
                    changed = true;
                }
            }
            if changed {
                let _ = db.collection::<Character>(CHARACTERS_COLLECTION_NAME)
                    .replace_one(doc! { "_id": _character._id }, _character.clone())
                    .await;
            }
        } else {return Err("resolve_stat__character_not_found".into())}

        if let Some(ref mut road) = road {
            let initial_len = road.modifiers.len();
            road.modifiers.retain(|m| m.is_active());
            if road.modifiers.len() != initial_len {
                let _ = db.collection::<Road>(ROADS_COLLECTION_NAME)
                    .replace_one(doc! { "_id": road._id }, road.clone())
                    .await;
            }
        }

        if let Some(ref mut _place) = place {
            let initial_len = _place.modifiers.len();
            _place.modifiers.retain(|m| m.is_active());
            if _place.modifiers.len() != initial_len {
                let _ = _place.update().await;
            }
        }

        if let Some(ref mut _universe_stat) = universe_stat {
            let initial_len = _universe_stat.modifiers.len();
            _universe_stat.modifiers.retain(|m| m.is_active());
            if _universe_stat.modifiers.len() != initial_len {
                let _ = db.collection::<Stat>(STATS_COLLECTION_NAME)
                    .replace_one(doc! { "_id": _universe_stat._id }, _universe_stat.clone())
                    .await;
            }
        }



        let mut grouped_modifiers: Vec<Modifier> = vec![];

        let mut apply_modifiers = |modifiers: &Vec<Modifier>, multipliers: &mut f64, bases: &mut f64, flats: &mut f64| -> bool {
            grouped_modifiers.append(modifiers.clone().as_mut());
            let mut local_has_multiplier = false;
            let mut multiplier_sum = 0.0;
            
            for modifier in modifiers {
                if modifier.stat_id == stat_id {
                    match modifier.modifier_type {
                        ModifierType::Multiplier => {
                            multiplier_sum += modifier.value.as_f64();
                            local_has_multiplier = true;
                        },
                        ModifierType::Base => *bases += modifier.value.as_f64(),
                        ModifierType::Flats => *flats += modifier.value.as_f64(),
                    }
                }
            }
            
            if local_has_multiplier {
                *multipliers = multiplier_sum;
            }
            local_has_multiplier
        };

        let mut value = self.base_value.as_f64();

        // Ordre d'application : Joueur -> Espace -> Lieu -> Univers
        // Formule à chaque étape : a(x + b) + c
        // a = Multiplier, b = Base, c = Flats

        // 1. Joueur
        if let Some(_character) = character {
            if let Some(c_stat) = _character.stats.iter().find(|s| s.name == self.name) {
                let mut multipliers = 1.0;
                let mut bases = 0.0;
                let mut flats = 0.0;
                let has_multiplier = apply_modifiers(&c_stat.modifiers, &mut multipliers, &mut bases, &mut flats);
                
                // x = c_stat.base_value
                value = multipliers * (c_stat.base_value.as_f64() + bases) + flats;
            } else {
                // Si le personnage n'a pas la stat, on utilise la base de la stat globale
                let mut multipliers = 1.0;
                let mut bases = 0.0;
                let mut flats = 0.0;
                // On peut quand même avoir des modificateurs globaux sur le joueur pour cette stat ?
                // Actuellement apply_modifiers prend une liste. 
                // Dans le code original, il semble que si c_stat n'existe pas, on ne faisait rien.
                // Mais l'ordre demande Joueur en premier.
                let _ = apply_modifiers(&vec![], &mut multipliers, &mut bases, &mut flats);
                value = multipliers * (value + bases) + flats;
            }
        } else {
            return Err("resolve_stat__character_not_found".into());
        }

        // 2. Espace (Salon ou Route)
        if let Some(ref _road) = road {
            let mut multipliers = 1.0;
            let mut bases = 0.0;
            let mut flats = 0.0;
            let _ = apply_modifiers(&_road.modifiers, &mut multipliers, &mut bases, &mut flats);
            value = multipliers * (value + bases) + flats;
        } else if let Some(ref _place) = place {
            // Si on est dans un salon (Place) et pas sur une route
            let mut multipliers = 1.0;
            let mut bases = 0.0;
            let mut flats = 0.0;
            let _ = apply_modifiers(&_place.modifiers, &mut multipliers, &mut bases, &mut flats);
            value = multipliers * (value + bases) + flats;
        }

        // 3. Lieu (Catégorie ou Route)
        // La demande dit : "lieu (catégorie ou route)".
        // Si on est sur une route, l'espace ET le lieu sont la route ? 
        // Ou l'espace est la route et le lieu est... ?
        // Dans le doute, si c'est une route on réapplique les modificateurs de la route ?
        // "joueur; espace (salon ou route); lieu (catégorie ou route); univers."
        // Si c'est une route, on applique l'étape 2 (route) puis l'étape 3 (route encore ? ou catégorie parente ?)
        // Les routes n'ont pas de catégorie parente directe dans leur structure.
        // Si c'est un salon (Place), l'espace est le salon, et le lieu est la catégorie.
        // Mais Place REPRÉSENTE déjà la catégorie (category_id).
        // On va supposer que pour une Place, Espace = Place, Lieu = Place (ou on saute si c'est le même).
        // Mais pour suivre l'ordre 4 étapes :
        if let Some(_road) = road {
            let mut multipliers = 1.0;
            let mut bases = 0.0;
            let mut flats = 0.0;
            let _ = apply_modifiers(&_road.modifiers, &mut multipliers, &mut bases, &mut flats);
            value = multipliers * (value + bases) + flats;
        } else if let Some(_place) = place {
            let mut multipliers = 1.0;
            let mut bases = 0.0;
            let mut flats = 0.0;
            let _ = apply_modifiers(&_place.modifiers, &mut multipliers, &mut bases, &mut flats);
            value = multipliers * (value + bases) + flats;
        }

        // 4. Univers
        if let Some(u_stat) = universe_stat {
            let mut multipliers = 1.0;
            let mut bases = 0.0;
            let mut flats = 0.0;
            let has_multiplier = apply_modifiers(&u_stat.modifiers, &mut multipliers, &mut bases, &mut flats);

            // Appliquer également le global_time_modifier de l'univers si c'est la stat de vitesse
            if self.name == SPEED_STAT {
                if let Ok(Some(universe)) = crate::database::universe::get_universe_by_id(self.universe_id).await {
                    let universe_mult = (universe.global_time_modifier as f64) / 100.0;
                    if !has_multiplier {
                        multipliers = universe_mult;
                    } else {
                        multipliers += universe_mult;
                    }
                }
            }

            value = multipliers * (value + bases) + flats;
        } else if self.name == SPEED_STAT {
            // Si pas de stat globale mais que c'est la vitesse, on applique quand même le modificateur d'univers
            if let Ok(Some(universe)) = crate::database::universe::get_universe_by_id(self.universe_id).await {
                value *= (universe.global_time_modifier as f64) / 100.0;
            }
        }

        if let Some(max) = self.max { match max{
            StatValue::I64(max) => {if (max as f64) < value {value = max as f64}}
            StatValue::F64(max) => {if max < value {value = max}}
            _ => {}
        }}
        if let Some(min) = self.min { match min{
            StatValue::I64(min) => {if min as f64 > value {value = min as f64}}
            StatValue::F64(min) => {if min > value {value = min}}
            _ => {}
        }}

        let shortest_modifier_end_timestamp = grouped_modifiers.clone().into_iter().min_by_key(|modifier| modifier.end_timestamp);

        match self.base_value {
            StatValue::I64(_) => Ok((StatValue::I64(value.round() as i64), shortest_modifier_end_timestamp)),
            _ => Ok((StatValue::F64(value), shortest_modifier_end_timestamp)),
        }
    }
    pub async fn get_stat_by_id(id: ObjectId) -> mongodb::error::Result<Option<Stat>> {
        let db_client = get_db_client().await;
        db_client.database(VERSEENGINE_DB_NAME)
            .collection::<Stat>(STATS_COLLECTION_NAME)
            .find_one(doc! { "_id": id })
            .await
    }
}

pub async fn get_stat_by_name(universe_id: ObjectId, name: &str) -> mongodb::error::Result<Option<Stat>> {
    let db_client = get_db_client().await;
    db_client.database(VERSEENGINE_DB_NAME)
        .collection::<Stat>(STATS_COLLECTION_NAME)
        .find_one(doc! { "name": name, "universe_id":  universe_id })
        .await
}