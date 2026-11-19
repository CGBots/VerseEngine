//! Objets renvoyés au client. Tous les identifiants sont des chaînes (précision JS).

use serde::Serialize;
use serde_json::Value;

use crate::activity::travel_view::TravelView;
use crate::database::modifiers::{Modifier, ModifierLevel, ModifierType};
use crate::database::stats::StatValue;

pub fn stat_value_json(v: &StatValue) -> Value {
    match v {
        StatValue::I64(i) => Value::from(*i),
        StatValue::F64(f) => serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        StatValue::String(s) => Value::from(s.clone()),
        StatValue::Bool(b) => Value::from(*b),
    }
}

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UniverseDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct StatDto {
    pub id: String,
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Serialize)]
pub struct StatusDto {
    pub stat: String,
    pub value: Value,
    pub modifier_type: &'static str,
    pub level: &'static str,
    pub end_timestamp: Option<u64>,
    pub source: String,
}

impl StatusDto {
    pub fn from_modifier(stat_name: &str, m: &Modifier) -> Self {
        Self {
            stat: stat_name.to_owned(),
            value: stat_value_json(&m.value),
            modifier_type: match m.modifier_type {
                ModifierType::Base => "base",
                ModifierType::Multiplier => "multiplier",
                ModifierType::Flats => "flat",
            },
            level: match m.level {
                ModifierLevel::Player => "player",
                ModifierLevel::Place => "place",
                ModifierLevel::Area => "area",
                ModifierLevel::Universe => "universe",
            },
            end_timestamp: m.end_timestamp,
            source: m.source.to_hex(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LocationDto {
    /// "place" ou "road".
    pub kind: &'static str,
    /// `category_id` du lieu ou `channel_id` de la route.
    pub id: String,
    pub name: Option<String>,
    /// Lieu de départ (sur une route), pour placer le groupe sur la carte.
    pub source_id: Option<String>,
    pub destination_id: Option<String>,
    pub destination_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CharacterDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub speed: Option<f64>,
    pub stats: Vec<StatDto>,
    pub statuses: Vec<StatusDto>,
    pub location: Option<LocationDto>,
    pub travel: Option<TravelView>,
    pub group_members: Vec<String>,
    pub server_time: u64,
}
