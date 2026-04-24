use mongodb::bson::oid::ObjectId;
use crate::database::modifiers::{Modifier, ModifierType, ModifierLevel};
use crate::database::stats::{get_stat_by_name, StatValue};

pub struct EffectParser;

impl EffectParser {
    /// Parsea une chaîne de caractères représentant des effets.
    /// Syntaxe: Stat: Valeur[Type] Durée Niveau
    /// Exemple: Force: +5 10m joueur
    ///          Vitesse: x1.2 1h endroit
    ///          HP: 10 flat univers
    pub async fn parse(text: &str, universe_id: ObjectId, source_id: ObjectId) -> Result<Vec<Modifier>, String> {
        let mut modifiers = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 2 {
                continue; // Ligne invalide ou pas un effet
            }

            let stat_name = parts[0].trim();
            let remainder = parts[1].trim();

            let stat = get_stat_by_name(universe_id, stat_name).await
                .map_err(|e| format!("Erreur DB lors de la recherche de la stat {}: {}", stat_name, e))?
                .ok_or_else(|| format!("Stat non trouvée: {}", stat_name))?;

            // Découpage du reste par espaces
            let words: Vec<&str> = remainder.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }

            let value_str = words[0];
            let mut duration_str = None;
            let mut level = ModifierLevel::Player;

            // On parcourt les mots restants pour trouver la durée et le niveau
            for &word in words.iter().skip(1) {
                if let Some(lvl) = Self::try_parse_level(word) {
                    level = lvl;
                } else if Self::is_duration(word) {
                    duration_str = Some(word);
                } else if word.to_lowercase() == "flat" {
                    // Si le mot est "flat", on l'ajoute à la valeur si ce n'est pas déjà fait
                    // Note: parse_value gère déjà "flat" à la fin de la chaîne
                }
            }

            // Si "flat" est un mot séparé, on le recolle pour parse_value
            let mut full_value_str = value_str.to_string();
            if words.iter().skip(1).any(|&w| w.to_lowercase() == "flat") && !value_str.to_lowercase().ends_with("flat") {
                full_value_str.push_str(" flat");
            }

            let end_timestamp = if let Some(d) = duration_str {
                Some(Self::parse_duration_to_seconds(d)?)
            } else {
                None
            };

            let (val, mod_type) = Self::parse_value(&full_value_str)?;

            modifiers.push(Modifier {
                stat_id: stat._id,
                value: val,
                modifier_type: mod_type,
                end_timestamp,
                source: source_id,
                level,
            });
        }

        Ok(modifiers)
    }

    fn try_parse_level(s: &str) -> Option<ModifierLevel> {
        match s.to_lowercase().as_str() {
            "joueur" | "player" => Some(ModifierLevel::Player),
            "endroit" | "place" | "salon" => Some(ModifierLevel::Place),
            "lieu" | "area" | "catégorie" | "categorie" => Some(ModifierLevel::Area),
            "univers" | "universe" => Some(ModifierLevel::Universe),
            _ => None,
        }
    }

    fn is_duration(s: &str) -> bool {
        let s = s.to_lowercase();
        if s.is_empty() { return false; }
        let has_digit = s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        let has_unit = s.ends_with('s') || s.ends_with('m') || s.ends_with('h') || s.ends_with('d') || s.ends_with('j');
        has_digit && has_unit
    }

    fn parse_value(s: &str) -> Result<(StatValue, ModifierType), String> {
        if s.starts_with('x') || s.starts_with('*') {
            let val = s[1..].parse::<f64>().map_err(|_| format!("Valeur multiplicateur invalide: {}", s))?;
            Ok((StatValue::F64(val), ModifierType::Multiplier))
        } else if s.starts_with('+') {
            let val = s[1..].parse::<f64>().map_err(|_| format!("Valeur addition invalide: {}", s))?;
            Ok((StatValue::F64(val), ModifierType::Base))
        } else if s.to_lowercase().ends_with("flat") {
            let val_str = s[..s.len()-4].trim();
            let val = val_str.parse::<f64>().map_err(|_| format!("Valeur flat invalide: {}", s))?;
            Ok((StatValue::F64(val), ModifierType::Flats))
        } else {
            // Par défaut on considère que c'est une addition si c'est un nombre
            if let Ok(val) = s.parse::<f64>() {
                Ok((StatValue::F64(val), ModifierType::Base))
            } else {
                // Sinon on traite comme un Flat String ? 
                // Pour l'instant on reste sur du numérique pour les effets classiques
                Err(format!("Format de valeur inconnu: {}", s))
            }
        }
    }

    fn parse_duration_to_seconds(d: &str) -> Result<u64, String> {
        let d = d.trim().to_lowercase();

        let num_str: String = d.chars().take_while(|c| c.is_digit(10)).collect();
        let unit_str: String = d.chars().skip_while(|c| c.is_digit(10)).collect();

        if num_str.is_empty() {
            return Err(format!("Durée invalide: {}", d));
        }

        let num = num_str.parse::<u64>().map_err(|_| format!("Nombre invalide dans la durée: {}", d))?;
        
        let multiplier = match unit_str.trim() {
            "s" => 1,
            "m" => 60,
            "h" => 3600,
            "d" | "j" => 86400,
            "" => 60, // Par défaut minutes ?
            _ => return Err(format!("Unité de temps inconnue: {}", unit_str)),
        };

        Ok(num * multiplier)
    }
}
