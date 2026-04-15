use mongodb::bson::oid::ObjectId;

pub struct Recipe {
    _id : ObjectId,
    tool_id: Option<ObjectId>,
    ingredients: Vec<(u64, ObjectId)>,
    result: Vec<(u64, ObjectId)>,
    delay: u64,
}
