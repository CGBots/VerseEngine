//! # Tables de butin : validation et enregistrement partagés
//!
//! `/loot_table edit` (texte saisi dans une modale, lu par [`LootTableParser`]) et l'éditeur de
//! l'activité (entrées structurées) enregistrent par [`save_table`] : même document, même
//! conservation des dernières fouilles (`last_loot`). L'éditeur de l'activité régénère aussi le
//! texte ([`to_raw_text`]) pour que la modale de la commande slash montre la table à jour.
//!
//! [`LootTableParser`]: crate::utility::loot_table_parser::LootTableParser

use std::collections::HashSet;

use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::database::loot_tables::{get_loot_table_by_channel_id, LootTable, LootTableEntry, LootTableItem, LootTableSet};

/// Entrées d'une table (au-delà, le tirage et la modale deviennent illisibles).
pub const MAX_ENTRIES: usize = 100;
/// Objets d'un ensemble.
pub const MAX_SET_ITEMS: usize = 50;
pub const MAX_NAME_CHARS: usize = 100;
/// Quantité max tirée d'un coup : le tirage pousse un nom par exemplaire, une borne évite
/// qu'une faute de frappe (1-1000000) ne fasse exploser la mémoire.
pub const MAX_QUANTITY: u32 = 1000;
/// Nombre max de tirages dans un ensemble.
pub const MAX_PICKS: u32 = 100;
pub const MAX_STOCK: u32 = 1_000_000;
/// Délai et recharge : un an au plus (secondes).
pub const MAX_SECONDS: u64 = 365 * 24 * 3600;

/// Une entrée telle que l'éditeur l'envoie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntryInput {
    Item(ItemInput),
    Set {
        name: String,
        probability: f64,
        min: u32,
        max: u32,
        #[serde(default)]
        stock: Option<u32>,
        #[serde(default)]
        secret: bool,
        items: Vec<ItemInput>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInput {
    pub name: String,
    /// Probabilité (en %) pour une entrée ; poids relatif dans un ensemble.
    pub probability: f64,
    pub min: u32,
    pub max: u32,
    #[serde(default)]
    pub stock: Option<u32>,
    #[serde(default)]
    pub secret: bool,
}

/// Table envoyée par l'éditeur.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableInput {
    pub entries: Vec<EntryInput>,
    #[serde(default)]
    pub rate_limit: Option<u64>,
    #[serde(default)]
    pub delay: Option<u64>,
}

/// Erreur de validation, avec l'endroit fautif (indices à partir de 0) pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub rule: Rule,
    pub entry: Option<usize>,
    pub item: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    TooManyEntries,
    TooManySetItems,
    EmptySet,
    EmptyName,
    NameTooLong,
    /// `:` ou retour à la ligne, ou `-` en tête : illisible pour la commande slash.
    NameNotWritable,
    UnknownItem,
    Probability,
    MinMax,
    Quantity,
    Stock,
    Seconds,
}

impl Invalid {
    fn at(rule: Rule, entry: Option<usize>, item: Option<usize>) -> Self { Self { rule, entry, item } }
}

fn check_name(name: &str, rule_at: impl Fn(Rule) -> Invalid) -> Result<String, Invalid> {
    let name = name.trim();
    if name.is_empty() {
        return Err(rule_at(Rule::EmptyName));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(rule_at(Rule::NameTooLong));
    }
    if name.contains(':') || name.contains('\n') || name.contains('\r') || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err(rule_at(Rule::NameNotWritable));
    }
    Ok(name.to_string())
}

/// Probabilité d'une entrée (%) ou poids dans un ensemble : fini, dans `]0, 100]`.
fn check_probability(p: f64) -> bool { p.is_finite() && p > 0.0 && p <= 100.0 }

fn check_item(it: &ItemInput, known: &HashSet<String>, at: impl Fn(Rule) -> Invalid + Copy) -> Result<LootTableItem, Invalid> {
    let name = check_name(&it.name, at)?;
    if !known.contains(&name) {
        return Err(at(Rule::UnknownItem));
    }
    if !check_probability(it.probability) {
        return Err(at(Rule::Probability));
    }
    // `roll` tire `min..=max` : min > max ferait paniquer le tirage.
    if it.min > it.max {
        return Err(at(Rule::MinMax));
    }
    if it.max == 0 || it.max > MAX_QUANTITY {
        return Err(at(Rule::Quantity));
    }
    if it.stock.is_some_and(|s| s == 0 || s > MAX_STOCK) {
        return Err(at(Rule::Stock));
    }
    Ok(LootTableItem { name, probability: it.probability, min: it.min, max: it.max, stock: it.stock, secret: it.secret })
}

/// Valide les entrées de l'éditeur. `known` : noms des objets de l'univers (un objet absent ne
/// serait jamais donné : `loot_table__item_not_found` à chaque fouille).
pub fn validate(input: &TableInput, known: &HashSet<String>) -> Result<Vec<LootTableEntry>, Invalid> {
    if input.entries.len() > MAX_ENTRIES {
        return Err(Invalid::at(Rule::TooManyEntries, None, None));
    }
    for s in [input.rate_limit, input.delay].into_iter().flatten() {
        if s > MAX_SECONDS {
            return Err(Invalid::at(Rule::Seconds, None, None));
        }
    }
    let mut out = Vec::with_capacity(input.entries.len());
    for (e, entry) in input.entries.iter().enumerate() {
        match entry {
            EntryInput::Item(it) => out.push(LootTableEntry::Item(check_item(it, known, |r| Invalid::at(r, Some(e), None))?)),
            EntryInput::Set { name, probability, min, max, stock, secret, items } => {
                let at = |r| Invalid::at(r, Some(e), None);
                let name = check_name(name, at)?;
                if !check_probability(*probability) {
                    return Err(at(Rule::Probability));
                }
                if min > max {
                    return Err(at(Rule::MinMax));
                }
                if *max == 0 || *max > MAX_PICKS {
                    return Err(at(Rule::Quantity));
                }
                if stock.is_some_and(|s| s == 0 || s > MAX_STOCK) {
                    return Err(at(Rule::Stock));
                }
                // Un ensemble vide est retiré au premier tirage : inutile de l'enregistrer.
                if items.is_empty() {
                    return Err(at(Rule::EmptySet));
                }
                if items.len() > MAX_SET_ITEMS {
                    return Err(at(Rule::TooManySetItems));
                }
                let items = items
                    .iter()
                    .enumerate()
                    .map(|(i, it)| check_item(it, known, |r| Invalid::at(r, Some(e), Some(i))))
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(LootTableEntry::Set(LootTableSet { name, probability: *probability, min: *min, max: *max, stock: *stock, items, secret: *secret }));
            }
        }
    }
    Ok(out)
}

/// Entrées enregistrées → format de l'éditeur.
pub fn to_input(entries: &[LootTableEntry]) -> Vec<EntryInput> {
    let item = |i: &LootTableItem| ItemInput { name: i.name.clone(), probability: i.probability, min: i.min, max: i.max, stock: i.stock, secret: i.secret };
    entries
        .iter()
        .map(|e| match e {
            LootTableEntry::Item(i) => EntryInput::Item(item(i)),
            LootTableEntry::Set(s) => EntryInput::Set {
                name: s.name.clone(),
                probability: s.probability,
                min: s.min,
                max: s.max,
                stock: s.stock,
                secret: s.secret,
                items: s.items.iter().map(item).collect(),
            },
        })
        .collect()
}

fn params(probability: f64, min: u32, max: u32, stock: Option<u32>, secret: bool) -> String {
    let mut out = format!("{probability}, ");
    if min == max {
        out.push_str(&min.to_string());
    } else {
        out.push_str(&format!("{min}-{max}"));
    }
    if let Some(s) = stock {
        out.push_str(&format!(", stock:{s}"));
    }
    if secret {
        out.push_str(", secret");
    }
    out
}

/// Texte de la table au format de la modale `/loot_table edit` (`nom: prob, min-max, stock:N,
/// secret`, objets d'un ensemble précédés de `-`).
pub fn to_raw_text(entries: &[LootTableEntry]) -> String {
    let mut lines = Vec::new();
    for e in entries {
        match e {
            LootTableEntry::Item(i) => lines.push(format!("{}: {}", i.name, params(i.probability, i.min, i.max, i.stock, i.secret))),
            LootTableEntry::Set(s) => {
                lines.push(format!("{}: {}", s.name, params(s.probability, s.min, s.max, s.stock, s.secret)));
                for i in &s.items {
                    lines.push(format!("- {}: {}", i.name, params(i.probability, i.min, i.max, i.stock, i.secret)));
                }
            }
        }
    }
    lines.join("\n")
}

/// Enregistre la table du salon (ou de la catégorie) `channel_id`, en gardant les dernières
/// fouilles de chaque personnage. `rate_limit` et `delay` à `None` reprennent ceux de la table
/// existante (comportement de la commande slash, dont les options sont facultatives).
pub async fn save_table(
    universe_id: ObjectId,
    channel_id: u64,
    entries: Vec<LootTableEntry>,
    raw_text: String,
    rate_limit: Option<u64>,
    delay: Option<u64>,
) -> mongodb::error::Result<LootTable> {
    let existing = get_loot_table_by_channel_id(universe_id, channel_id).await?;
    let table = LootTable {
        _id: None,
        universe_id,
        channel_id,
        entries,
        raw_text,
        rate_limit: rate_limit.or(existing.as_ref().and_then(|t| t.rate_limit)),
        delay: delay.or(existing.as_ref().and_then(|t| t.delay)),
        last_loot: existing.and_then(|t| t.last_loot),
    };
    table.save_or_update().await?;
    Ok(table)
}

/// Remplace la table telle que l'éditeur l'envoie : ici `None` veut dire « aucun » (le champ est
/// affiché et vidé par l'admin). Sans entrée, la table est supprimée : une table vide serait
/// effacée à la première fouille, avec un message « table vidée » trompeur dans les logs.
pub async fn replace_table(universe_id: ObjectId, channel_id: u64, input: &TableInput, entries: Vec<LootTableEntry>) -> mongodb::error::Result<()> {
    let existing = get_loot_table_by_channel_id(universe_id, channel_id).await?;
    if entries.is_empty() {
        if let Some(t) = existing {
            t.delete().await?;
        }
        return Ok(());
    }
    let table = LootTable {
        _id: None,
        universe_id,
        channel_id,
        raw_text: to_raw_text(&entries),
        entries,
        rate_limit: input.rate_limit.filter(|s| *s > 0),
        delay: input.delay.filter(|s| *s > 0),
        last_loot: existing.and_then(|t| t.last_loot),
    };
    table.save_or_update().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utility::loot_table_parser::LootTableParser;

    fn known() -> HashSet<String> { ["or", "plastron", "jambiere", "Épée"].into_iter().map(String::from).collect() }

    fn item(name: &str, p: f64, min: u32, max: u32) -> ItemInput { ItemInput { name: name.into(), probability: p, min, max, stock: None, secret: false } }

    fn table(entries: Vec<EntryInput>) -> TableInput { TableInput { entries, rate_limit: None, delay: None } }

    fn set(items: Vec<ItemInput>) -> EntryInput {
        EntryInput::Set { name: "armure".into(), probability: 20.5, min: 1, max: 2, stock: Some(3), secret: true, items }
    }

    #[test]
    fn a_valid_table_is_accepted_and_names_are_trimmed() {
        let entries = validate(&table(vec![EntryInput::Item(item(" or ", 40.0, 5, 20)), set(vec![item("plastron", 5.2, 1, 1)])]), &known()).unwrap();
        assert_eq!(entries.len(), 2);
        let LootTableEntry::Item(i) = &entries[0] else { panic!() };
        assert_eq!((i.name.as_str(), i.min, i.max), ("or", 5, 20));
        let LootTableEntry::Set(s) = &entries[1] else { panic!() };
        assert_eq!((s.items.len(), s.stock, s.secret), (1, Some(3), true));
    }

    #[test]
    fn probabilities_must_be_in_zero_to_hundred() {
        for p in [0.0, -1.0, 100.5, f64::NAN, f64::INFINITY] {
            let e = validate(&table(vec![EntryInput::Item(item("or", p, 1, 1))]), &known()).unwrap_err();
            assert_eq!(e, Invalid { rule: Rule::Probability, entry: Some(0), item: None }, "{p}");
        }
        assert!(validate(&table(vec![EntryInput::Item(item("or", 100.0, 1, 1))]), &known()).is_ok());
    }

    #[test]
    fn min_must_not_exceed_max_and_quantities_are_bounded() {
        let e = validate(&table(vec![EntryInput::Item(item("or", 10.0, 3, 2))]), &known()).unwrap_err();
        assert_eq!(e.rule, Rule::MinMax);
        assert_eq!(validate(&table(vec![EntryInput::Item(item("or", 10.0, 0, 0))]), &known()).unwrap_err().rule, Rule::Quantity);
        assert_eq!(validate(&table(vec![EntryInput::Item(item("or", 10.0, 1, MAX_QUANTITY + 1))]), &known()).unwrap_err().rule, Rule::Quantity);
        let e = validate(&table(vec![set(vec![item("plastron", 5.0, 4, 1)])]), &known()).unwrap_err();
        assert_eq!(e, Invalid { rule: Rule::MinMax, entry: Some(0), item: Some(0) }, "a set item");
    }

    #[test]
    fn items_must_exist_in_the_universe() {
        let e = validate(&table(vec![EntryInput::Item(item("or", 10.0, 1, 1)), set(vec![item("plastron", 1.0, 1, 1), item("inconnu", 1.0, 1, 1)])]), &known()).unwrap_err();
        assert_eq!(e, Invalid { rule: Rule::UnknownItem, entry: Some(1), item: Some(1) });
        // Le nom d'un ensemble n'est pas un objet.
        assert!(validate(&table(vec![set(vec![item("or", 1.0, 1, 1)])]), &known()).is_ok());
    }

    #[test]
    fn sets_need_items_and_writable_names() {
        assert_eq!(validate(&table(vec![set(vec![])]), &known()).unwrap_err().rule, Rule::EmptySet);
        let mut bad = set(vec![item("or", 1.0, 1, 1)]);
        if let EntryInput::Set { name, .. } = &mut bad {
            *name = "a: b".into();
        }
        assert_eq!(validate(&table(vec![bad]), &known()).unwrap_err().rule, Rule::NameNotWritable);
        assert_eq!(validate(&table(vec![EntryInput::Item(item("  ", 1.0, 1, 1))]), &known()).unwrap_err().rule, Rule::EmptyName);
    }

    #[test]
    fn stock_and_seconds_are_bounded() {
        let mut it = item("or", 1.0, 1, 1);
        it.stock = Some(0);
        assert_eq!(validate(&table(vec![EntryInput::Item(it)]), &known()).unwrap_err().rule, Rule::Stock);
        let mut t = table(vec![]);
        t.delay = Some(MAX_SECONDS + 1);
        assert_eq!(validate(&t, &known()).unwrap_err().rule, Rule::Seconds);
        let too_many = table(vec![EntryInput::Item(item("or", 1.0, 1, 1)); MAX_ENTRIES + 1]);
        assert_eq!(validate(&too_many, &known()).unwrap_err().rule, Rule::TooManyEntries);
    }

    #[test]
    fn the_editor_format_rejects_unknown_fields() {
        let ok = r#"{"entries":[{"kind":"item","name":"or","probability":40,"min":1,"max":2},{"kind":"set","name":"s","probability":5,"min":1,"max":1,"items":[{"name":"or","probability":1,"min":1,"max":1,"stock":2,"secret":true}]}],"delay":60}"#;
        let t: TableInput = serde_json::from_str(ok).unwrap();
        assert_eq!(t.entries.len(), 2);
        assert!(serde_json::from_str::<TableInput>(r#"{"entries":[],"admin":true}"#).is_err());
        assert!(serde_json::from_str::<TableInput>(r#"{"entries":[{"kind":"item","name":"or","probability":1,"min":1,"max":1,"x":1}]}"#).is_err());
        assert!(serde_json::from_str::<TableInput>(r#"{"entries":[{"kind":"item","name":"or","probability":1,"min":-1,"max":1}]}"#).is_err());
    }

    #[tokio::test]
    async fn the_generated_text_is_read_back_by_the_slash_command_parser() {
        let input = table(vec![
            EntryInput::Item(ItemInput { name: "or".into(), probability: 40.0, min: 5, max: 20, stock: None, secret: false }),
            EntryInput::Item(ItemInput { name: "epee".into(), probability: 5.5, min: 1, max: 1, stock: Some(1), secret: true }),
            set(vec![item("plastron", 5.2, 1, 1), ItemInput { name: "jambiere".into(), probability: 5.0, min: 1, max: 2, stock: Some(2), secret: true }]),
        ]);
        let mut names = known();
        names.insert("epee".into());
        let entries = validate(&input, &names).unwrap();
        let text = to_raw_text(&entries);
        assert_eq!(text.lines().next(), Some("or: 40, 5-20"));
        let parsed = LootTableParser::parse_internal(&text, ObjectId::new(), false).await.unwrap();
        assert_eq!(to_input(&parsed), to_input(&entries));
        assert_eq!(to_input(&entries), input.entries.clone(), "the editor format round-trips");
    }
}
