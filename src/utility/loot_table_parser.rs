use std::sync::LazyLock;
use regex::Regex;
use crate::database::loot_tables::{LootTableEntry, LootTableItem, LootTableSet};
use crate::database::items::get_item_by_name;
use mongodb::bson::oid::ObjectId;

// Autoriser uniquement lettres/chiffres, underscore et espaces dans les noms
pub static VALID_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_ -]+$").unwrap()
});

pub struct LootTableParser;

impl LootTableParser {
    pub async fn parse(text: &str, universe_id: ObjectId) -> Result<Vec<LootTableEntry>, String> {
        Self::parse_internal(text, universe_id, true).await
    }

    async fn parse_internal(text: &str, universe_id: ObjectId, validate_db: bool) -> Result<Vec<LootTableEntry>, String> {
        let mut entries = Vec::new();
        let mut lines = text.lines().peekable();
        let mut current_set: Option<LootTableSet> = None;
        let mut pending_item: Option<LootTableItem> = None;

        while let Some(line) = lines.next() {
            let trimmed_line = line.trim();
            if trimmed_line.is_empty() {
                continue;
            }

            // Gestion des items de set (commençant par '-')
            if trimmed_line.starts_with('-') {
                let item_line = trimmed_line[1..].trim();
                let item = Self::parse_item_line(item_line, universe_id, validate_db).await
                    .map_err(|e| format!("Error in set item '{}': {}", item_line, e))?;

                if let Some(mut set) = current_set.take() {
                    // On est déjà dans un set
                    set.items.push(item);
                    current_set = Some(set);
                } else if let Some(p_item) = pending_item.take() {
                    // Transition Item -> Élément de set : le précédent item était en fait l'en-tête d'un set
                    let mut set = LootTableSet {
                        name: p_item.name,
                        probability: p_item.probability,
                        min: p_item.min,
                        max: p_item.max,
                        stock: p_item.stock,
                        items: Vec::new(),
                        secret: p_item.secret,
                    };
                    set.items.push(item);
                    current_set = Some(set);
                } else {
                    return Err(format!("Item found outside of a set: {}", trimmed_line));
                }
                continue;
            }

            // Si on arrive ici, on n'est plus dans un item de set.
            // On finalise ce qui est en cours (set ou item simple).
            if let Some(set) = current_set.take() {
                entries.push(LootTableEntry::Set(set));
            }
            if let Some(item) = pending_item.take() {
                // Si on a un item en attente, c'est qu'on a lu une nouvelle ligne qui n'est pas un élément de set.
                // Donc le pending_item est bien un item simple.
                if validate_db {
                    if get_item_by_name(universe_id, &item.name).await.map_err(|e| e.to_string())?.is_none() {
                        return Err(format!("Item not found: {}", item.name));
                    }
                }
                entries.push(LootTableEntry::Item(item));
            }

            // Nouvelle ligne d'item (potentiellement en-tête de set)
            let colon_pos = trimmed_line.find(':').ok_or_else(|| format!("Missing colon in line: {}", trimmed_line))?;
            let name = trimmed_line[..colon_pos].trim().to_string();
            if name.is_empty() {
                return Err(format!("Empty name in line: {}", trimmed_line));
            }
            if !VALID_NAME_RE.is_match(&name) {
                return Err(format!("loot_table__invalid_item_name:{}", name));
            }

            let params_str = trimmed_line[colon_pos + 1..].trim();
            let (prob, min, max, stock, secret) = Self::parse_params(params_str)
                .map_err(|e| format!("Error in line '{}' parameters: {}", name, e))?;

            // On stocke cet item en attente de voir la ligne suivante
            pending_item = Some(LootTableItem {
                name,
                probability: prob,
                min,
                max,
                stock,
                secret,
            });
        }

        // Finalisation à la fin du texte
        if let Some(set) = current_set {
            entries.push(LootTableEntry::Set(set));
        } else if let Some(item) = pending_item {
            // Validation finale pour le dernier item simple si demandée
            if validate_db {
                if get_item_by_name(universe_id, &item.name).await.map_err(|e| e.to_string())?.is_none() {
                    return Err(format!("Item not found: {}", item.name));
                }
            }
            entries.push(LootTableEntry::Item(item));
        }

        Ok(entries)
    }

    async fn parse_item_line(line: &str, universe_id: ObjectId, validate_db: bool) -> Result<LootTableItem, String> {
        let colon_pos = line.find(':').ok_or_else(|| format!("Missing colon in item line: {}", line))?;
        let name = line[..colon_pos].trim().to_string();
        if name.is_empty() {
            return Err("Empty item name".to_string());
        }
        if !VALID_NAME_RE.is_match(&name) {
            return Err(format!("loot_table__invalid_item_name:{}", name));
        }

        let params_str = line[colon_pos + 1..].trim();
        let (probability, min, max, stock, secret) = Self::parse_params(params_str)
            .map_err(|e| format!("Error in item '{}' parameters: {}", name, e))?;

        if validate_db {
            if get_item_by_name(universe_id, &name).await.map_err(|e| e.to_string())?.is_none() {
                return Err(format!("Item not found: {}", name));
            }
        }

        Ok(LootTableItem {
            name,
            probability,
            min,
            max,
            stock,
            secret,
        })
    }

    fn parse_params(params_str: &str) -> Result<(f64, u32, u32, Option<u32>, bool), String> {
        let parts: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();
        if parts.is_empty() || parts[0].is_empty() {
            return Err("Missing probability".to_string());
        }

        let probability = parts[0].parse::<f64>().map_err(|_| format!("Invalid probability: {}", parts[0]))?;

        let mut min = 1;
        let mut max = 1;
        let mut stock = None;
        let mut secret = false;

        for (i, part) in parts.iter().enumerate().skip(1) {
            if part.is_empty() { continue; }

            if part.starts_with("stock:") {
                stock = Some(part[6..].trim().parse::<u32>().map_err(|_| format!("Invalid stock: {}", part))?);
            } else if *part == "secret" {
                secret = true;
            } else if i == 1 {
                // Probablement le min-max s'il est en deuxième position et ne commence pas par stock:
                let (p_min, p_max) = Self::parse_min_max(part)?;
                min = p_min;
                max = p_max;
            } else {
                return Err(format!("Unknown parameter '{}'. Expected order: prob, quantity, stock:N, secret", part));
            }
        }

        Ok((probability, min, max, stock, secret))
    }

    fn parse_min_max(s: &str) -> Result<(u32, u32), String> {
        if let Some(pos) = s.find('-') {
            let min = s[..pos].trim().parse::<u32>().map_err(|_| format!("Invalid min value: {}", &s[..pos]))?;
            let max = s[pos + 1..].trim().parse::<u32>().map_err(|_| format!("Invalid max value: {}", &s[pos + 1..]))?;
            if min > max {
                return Err(format!("loot_table__invalid_min_max:{}|{}", min, max));
            }
            Ok((min, max))
        } else {
            let val = s.parse::<u32>().map_err(|_| format!("Invalid value: {}", s))?;
            Ok((val, val))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LootTableParser;
    use mongodb::bson::oid::ObjectId;
    use crate::database::loot_tables::LootTableEntry;

    // L'exemple de l'issue devrait maintenant fonctionner sans les crochets pour le set
    #[tokio::test]
    async fn parse_test_robustness() {
        let text = r#"
            or: 40, 5-20
            epee-legendaire : 5, 1, stock:1, secret

            armure-chevalier: 20.5, 1
            - plastron: 5.2, 1, stock:2
            - jambiere : 5, 1-2
            - gantelet: 5, 1-2, stock:1
            - grimoire: 1, 1, secret
        "#;
        
        let universe_id = ObjectId::new();
        // On désactive la validation DB pour les tests unitaires
        let result = LootTableParser::parse_internal(text, universe_id, false).await;
        
        match result {
            Err(e) => panic!("Parsing failed with syntax error: {}", e),
            Ok(entries) => {
                assert_eq!(entries.len(), 3);
                
                // Vérification de l'item simple avec tiret
                if let LootTableEntry::Item(item) = &entries[1] {
                    assert_eq!(item.name, "epee-legendaire");
                    assert_eq!(item.probability, 5.0);
                    assert_eq!(item.stock, Some(1));
                    assert!(item.secret);
                } else {
                    panic!("Expected Item at index 1");
                }

                // Vérification du set
                if let LootTableEntry::Set(set) = &entries[2] {
                    assert_eq!(set.name, "armure-chevalier");
                    assert_eq!(set.items.len(), 4);
                    assert_eq!(set.items[0].name, "plastron");
                    assert_eq!(set.items[0].stock, Some(2));
                    assert_eq!(set.items[1].name, "jambiere");
                    assert_eq!(set.items[2].name, "gantelet");
                    assert_eq!(set.items[2].min, 1);
                    assert_eq!(set.items[2].max, 2);
                    assert_eq!(set.items[3].name, "grimoire");
                    assert!(set.items[3].secret);
                } else {
                    panic!("Expected Set at index 2");
                }
            }
        }
    }

    #[tokio::test]
    async fn parse_should_fail_on_invalid_syntax() {
        let text = "item_sans_deux_points 10, 1";
        let universe_id = ObjectId::new();
        let result = LootTableParser::parse(text, universe_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing colon"));
    }

    #[tokio::test]
    async fn parse_test_last_item_is_pending() {
        let text = "item1: 10";
        let universe_id = ObjectId::new();
        let result = LootTableParser::parse_internal(text, universe_id, false).await.unwrap();
        assert_eq!(result.len(), 1);
        if let LootTableEntry::Item(item) = &result[0] {
            assert_eq!(item.name, "item1");
        } else {
            panic!("Expected Item");
        }
    }

    #[tokio::test]
    async fn parse_test_transitions() {
        let text = r#"
            Item1: 10
            Item2: 20
            - Sub2: 5
            Item3: 30
        "#;
        let universe_id = ObjectId::new();
        let result = LootTableParser::parse_internal(text, universe_id, false).await.unwrap();
        assert_eq!(result.len(), 3);
        
        match &result[0] {
            LootTableEntry::Item(i) => assert_eq!(i.name, "Item1"),
            _ => panic!("Expected Item1"),
        }
        match &result[1] {
            LootTableEntry::Set(s) => {
                assert_eq!(s.name, "Item2");
                assert_eq!(s.items.len(), 1);
                assert_eq!(s.items[0].name, "Sub2");
            },
            _ => panic!("Expected Set Item2"),
        }
        match &result[2] {
            LootTableEntry::Item(i) => assert_eq!(i.name, "Item3"),
            _ => panic!("Expected Item3"),
        }
    }
}
