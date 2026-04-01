use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::stats::StatValue;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ModifierType{
    Base,
    Multiplier,
    Flats
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Modifier{
    pub stat_id: ObjectId,
    pub value: StatValue,
    pub modifier_type: ModifierType,
    pub end_timestamp: Option<u64>,
    pub source: ObjectId
}

impl Modifier {
    pub fn is_active(&self) -> bool {
        if let Some(end) = self.end_timestamp {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();
            return now < end;
        }
        true
    }
}