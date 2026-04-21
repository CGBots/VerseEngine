use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, Colour};
use crate::translation::get_by_locale;

pub struct CarouselPage {
    pub title: String,
    pub description: String,
    pub fields: Vec<(String, String, bool)>,
    pub footer: String,
    pub color: Colour,
}

pub struct CarouselConfig {
    pub prefix: String,
    pub current_page: usize,
    pub total_pages: usize,
    pub metadata: Vec<String>, // Additional data to be stored in custom_id (separated by :)
}

pub fn create_carousel_embed(page: CarouselPage) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(page.title)
        .description(page.description)
        .footer(CreateEmbedFooter::new(page.footer))
        .color(page.color);

    for (name, value, inline) in page.fields {
        embed = embed.field(name, value, inline);
    }

    embed
}

pub fn create_carousel_components(config: CarouselConfig, locale: &str) -> Vec<CreateActionRow> {
    let mut buttons = Vec::new();
    let metadata_str = config.metadata.join(":");

    if config.total_pages > 1 {
        let prev_page = config.current_page.saturating_sub(1);
        let prev_id = format!("{}:prev:{}:{}", config.prefix, metadata_str, prev_page);
        let prev_button = CreateButton::new(prev_id)
            .label(get_by_locale(locale, "carousel__previous_button", None, None))
            .style(ButtonStyle::Primary)
            .disabled(config.current_page == 0);

        let next_page = config.current_page + 1;
        let next_id = format!("{}:next:{}:{}", config.prefix, metadata_str, next_page);
        let next_button = CreateButton::new(next_id)
            .label(get_by_locale(locale, "carousel__next_button", None, None))
            .style(ButtonStyle::Primary)
            .disabled(config.current_page >= config.total_pages - 1);

        buttons.push(prev_button);
        buttons.push(next_button);
    }

    let refresh_id = format!("{}:refresh:{}:{}", config.prefix, metadata_str, config.current_page);
    let refresh_button = CreateButton::new(refresh_id)
        .label(get_by_locale(locale, "carousel__refresh_button", None, None))
        .style(ButtonStyle::Secondary);

    buttons.push(refresh_button);

    vec![CreateActionRow::Buttons(buttons)]
}

/// Helper function to build a standard paginated text description
pub fn paginate_text(items: &[String], items_per_page: usize, empty_message: &str) -> Vec<String> {
    if items.is_empty() {
        return vec![empty_message.to_string()];
    }
    items.chunks(items_per_page)
        .map(|chunk| chunk.join("\n"))
        .collect()
}
