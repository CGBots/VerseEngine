use mongodb::bson::oid::ObjectId;
use crate::database::items::get_item_by_name;

pub struct RecipeParser;

#[derive(Debug, Clone)]
pub struct ParsedRecipe {
    pub ingredients: Vec<(u64, ObjectId)>,
    pub result: Vec<(u64, ObjectId)>,
    pub tools_needed: Vec<ObjectId>,
}

impl RecipeParser {
    /// Analyse le texte brut de la recette pour extraire les ingrédients, résultats et outils.
    ///
    /// # Syntaxe
    /// - `> [nom de l'item] [quantité]` : Item obtenu (résultat).
    /// - `< [nom de l'item] [quantité]` : Item utilisé (ingrédient).
    /// - `- [nom de l'item]` : Outil nécessaire (non consommé).
    pub async fn parse(text: &str, universe_id: ObjectId) -> Result<ParsedRecipe, String> {
        let mut ingredients = Vec::new();
        let mut result_items = Vec::new();
        let mut tools_needed = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('>') {
                let (name, qty) = Self::parse_line(&line[1..])?;
                let item = get_item_by_name(universe_id, &name).await
                    .map_err(|e| format!("Erreur DB: {}", e))?
                    .ok_or_else(|| format!("recipe__item_not_found:{}", name))?;
                result_items.push((qty, item._id));
            } else if line.starts_with('<') {
                let (name, qty) = Self::parse_line(&line[1..])?;
                let item = get_item_by_name(universe_id, &name).await
                    .map_err(|e| format!("Erreur DB: {}", e))?
                    .ok_or_else(|| format!("recipe__item_not_found:{}", name))?;
                ingredients.push((qty, item._id));
            } else if line.starts_with('-') {
                let name = line[1..].trim();
                let item = get_item_by_name(universe_id, name).await
                    .map_err(|e| format!("Erreur DB: {}", e))?
                    .ok_or_else(|| format!("recipe__item_not_found:{}", name))?;
                tools_needed.push(item._id);
            }
        }

        if ingredients.is_empty() && result_items.is_empty() {
             return Err("recipe__empty_recipe".to_string());
        }

        Ok(ParsedRecipe {
            ingredients,
            result: result_items,
            tools_needed,
        })
    }

    fn parse_line(content: &str) -> Result<(String, u64), String> {
        let parts: Vec<&str> = content.split_whitespace().collect();
        if parts.is_empty() {
            return Err("recipe__invalid_line".to_string());
        }

        // On suppose que le dernier élément peut être la quantité s'il est numérique
        if parts.len() > 1 {
            if let Ok(qty) = parts.last().unwrap().parse::<u64>() {
                let name = parts[..parts.len()-1].join(" ");
                return Ok((name, qty));
            }
        }

        Ok((parts.join(" "), 1))
    }
}
