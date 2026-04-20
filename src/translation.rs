//! Translation system for handling fluent (`.ftl`) resources and applying localized strings to commands.
//!
//! ## Overview
//! This module provides tools for managing, formatting, and applying localized translations
//! with Fluent. It supports concurrent memoization and fallback mechanisms, ensuring
//! that translations can be fetched efficiently for a wide variety of locales.
//!
//! ## Key Components
//! - [`Translations`]: Holds the main and locale-specific translation bundles.
//! - `tr!`: A macro for convenient string translation with argument support.
//! - [`format`]: Formats a Fluent message, resolving IDs, attributes, and arguments to a final string.
//! - [`get`]: Retrieves a localized translation string, falling back gracefully if not found.
//! - [`read_ftl`]: Loads `.ftl` translation files into memory.
//! - [`apply_translations`]: Applies translations to structured command definitions.
//! - [`smart_tr`]: Enriches translations by auto-resolving missing variables.
//!
//! ## Usage
//! This module primarily supports applications where localization for commands and messaging is necessary,
//! such as bots or internationalized software systems.
use std::collections::HashMap;
use std::path::Path;
use crate::{Context, Data, Error};
use fluent::{FluentArgs, FluentValue};
use fluent::bundle::FluentBundle;
use fluent::FluentResource;
use intl_memoizer::concurrent::IntlLangMemoizer;
use lazy_static::lazy_static;
use regex::Regex;

/// Type alias for a Fluent bundle with concurrent memoization
type Bundle = FluentBundle<FluentResource, IntlLangMemoizer>;

lazy_static!(
    pub static ref TRANSLATIONS: Translations = read_ftl().expect("failed to read translation files");
);

/// A structure that holds translation bundles for managing multilingual support.
///
/// The `Translations` struct consists of a primary `main` bundle along
/// with additional bundles stored in a key-value mapping for handling
/// various languages and locales.
///
/// # Fields
///
/// * `main` -
///   The primary [`Bundle`](crate::Bundle) used for translations. This bundle typically contains
///   the default set of language translations or the primary locale.
///
/// * `other` -
///   A collection of additional bundles stored in a [`HashMap`], where the key is a `String`
///   representing the locale or language identifier (e.g., `en-US`, `fr`, `es`),
///   and the value is a [`Bundle`](crate::Bundle) containing the corresponding localized translations.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use crate::Translations;
///
/// let main_bundle = Bundle::new();
/// let mut other_bundles: HashMap<String, Bundle> = HashMap::new();
/// other_bundles.insert("fr".to_string(), Bundle::new());
///
/// let translations = Translations {
///     main: main_bundle,
///     other: other_bundles,
/// };
///
/// assert!(translations.other.contains_key("fr"));
/// ```
pub struct Translations {
    pub main: Bundle,
    pub other: HashMap<String, Bundle>,
}

/// A macro for performing translations using Fluent-based argument substitution.
///
/// This macro provides a convenient way to localize strings based on an identifier (`$id`) and
/// optional arguments. It supports Fluent-style parameterized localization, allowing users to pass
/// key-value pairs for placeholders within the translation string. If a translation is not found
/// or an error occurs, the macro defaults to returning the untranslated identifier (`$id`).
///
/// # Syntax
/// ```
/// tr!(context, id);
/// tr!(context, id, argname: value, ...);
/// ```
///
/// - When called with just `context` and `id`, it attempts to fetch the associated
///   translation from the Fluent resources.
/// - When called with additional arguments in the form `argname: value`, it substitutes
///   placeholders in the translation with the specified values.
///
/// # Parameters
///
/// - `$ctx:expr`
///   The translation context, typically a structure or object containing locale and Fluent resource
///   configuration.
///
/// - `$id:expr`
///   The identifier for the translation resource.
///
/// - `$argname:ident`
///   (Optional) The name of a placeholder in the translation string.
///
/// - `$argvalue:expr`
///   (Optional) The value for the given placeholder.
///
/// # Returns
///
/// Returns a `String` that contains the localized version of the input identifier with
/// placeholders substituted. If the translation fails or the identifier is not found,
/// the untranslated identifier (`$id`) is returned.
///
/// # Examples
///
/// Basic usage:
/// ```rust
/// let translation = tr!(context, "hello_world");
/// assert_eq!(translation, "Hello, World!");
/// ```
///
/// With arguments:
/// ```rust
/// let translation = tr!(context, "welcome_user", username: "Alice");
/// assert_eq!(translation, "Welcome, Alice!");
/// ```
///
/// Fallback to identifier if translation not found:
/// ```
/// let translation = tr!(context, "unknown_key");
/// assert_eq!(translation, "unknown_key");
/// ```

#[macro_export]
macro_rules! tr {
    ( $ctx:expr, $id:expr $(, $argname:ident: $argvalue:expr )* $(,)? ) => {{
        #[allow(unused_mut)]
        let mut args = fluent::FluentArgs::new();
        $( args.set(stringify!($argname), $argvalue); )*
        $crate::translation::smart_tr($ctx, $id, Some(&args)).unwrap_or_else(|_| $id.to_string())
    }};
    ( $ctx:expr, $id:expr ) => {{
        $crate::translation::smart_tr($ctx, $id, None).unwrap_or_else(|_| $id.to_string())
    }};
}

/// A macro for performing translations using Fluent-based argument substitution with explicit locale.
///
/// This macro provides a way to localize strings based on a locale string (e.g., "en-US") and
/// an identifier (`$id`), with optional arguments. Unlike `tr!`, this macro works with serenity
/// contexts that don't support the `smart_tr` function, requiring only a translations reference
/// and a locale string.
///
/// # Syntax
/// ```
/// tr_locale!(translations, locale, id);
/// tr_locale!(translations, locale, id, argname: value, ...);
/// ```
///
/// - When called with `translations`, `locale`, and `id`, it attempts to fetch the associated
///   translation from the Fluent resources for that locale.
/// - When called with additional arguments in the form `argname: value`, it substitutes
///   placeholders in the translation with the specified values.
///
/// # Parameters
///
/// - `$translations:expr`
///   A reference to the `Translations` structure containing translation bundles.
///
/// - `$locale:expr`
///   The locale string (e.g., "en-US", "fr", "es") identifying which translation to use.
///
/// - `$id:expr`
///   The identifier for the translation resource.
///
/// - `$argname:ident`
///   (Optional) The name of a placeholder in the translation string.
///
/// - `$argvalue:expr`
///   (Optional) The value for the given placeholder.
///
/// # Returns
///
/// Returns a `String` that contains the localized version of the input identifier with
/// placeholders substituted. If the translation fails or the identifier is not found,
/// the untranslated identifier (`$id`) is returned.
///
/// # Examples
///
/// Basic usage:
/// ```rust
/// let translation = tr_locale!(&translations, "en-US", "hello_world");
/// assert_eq!(translation, "Hello, World!");
/// ```
///
/// With arguments:
/// ```rust
/// let translation = tr_locale!(&translations, "en-US", "welcome_user", username: "Alice");
/// assert_eq!(translation, "Welcome, Alice!");
/// ```
///
/// Fallback to identifier if translation not found:
/// ```
/// let translation = tr_locale!(&translations, "en-US", "unknown_key");
/// assert_eq!(translation, "unknown_key");
/// ```
#[macro_export]
macro_rules! tr_locale {
    ( $locale:expr, $id:expr $(, $argname:ident: $argvalue:expr )* $(,)? ) => {{
        #[allow(unused_mut)]
        let mut args = fluent::FluentArgs::new();
        $( args.set(stringify!($argname), $argvalue); )*
        $crate::translation::get_by_locale($locale, $id, None, Some(&args))
    }};
    ( $locale:expr, $id:expr ) => {{
        $crate::translation::get_by_locale($locale, $id, None, None)
    }};
}
#[allow(unused_imports)]
pub(crate) use tr;



/// Formats a Fluent message or attribute into a localized string.
///
/// This function retrieves a message using its ID and optionally fetches
/// an associated attribute if specified. It then formats the corresponding
/// pattern using the provided arguments, if any, into a `String`.
///
/// # Parameters
///
/// - `bundle`: A reference to the `Bundle` containing the Fluent messages.
/// - `id`: The identifier of the message to format.
/// - `attr`: An optional attribute name used to fetch a specific attribute
///   of the message. If `None`, the message's value is used.
/// - `args`: An optional set of arguments (`FluentArgs`) for use in the
///   message or attribute's pattern.
///
/// # Returns
///
/// - `Some(String)`: The formatted localized string if the message (and
///   attribute, if specified) is found and formatting succeeds.
/// - `None`: If the message or attribute is missing or if formatting fails.
///
/// # Examples
///
/// ```rust
/// use fluent_bundle::{FluentBundle, FluentResource, FluentArgs};
/// use your_crate::format;
///
/// let res = FluentResource::try_new("
/// hello-world = Hello, { $name }!
/// ".to_string()).unwrap();
///
/// let mut bundle = FluentBundle::default();
/// bundle.add_resource(res).unwrap();
///
/// let mut args = FluentArgs::new();
/// args.set("name", "Alice".into());
///
/// let result = format(&bundle, "hello-world", None, Some(&args));
/// assert_eq!(result, Some("Hello, Alice!".to_string()));
/// ```
///
/// # Notes
///
/// This function assumes that the `Bundle` is properly configured with the relevant
/// Fluent resources and that the `id` and `attr` (if provided) correspond to valid
/// entries in those resources.
pub fn format(
    bundle: &Bundle,
    id: &str,
    attr: Option<&str>,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let message = bundle.get_message(id)?;
    let pattern = match attr {
        Some(attribute) => message.get_attribute(attribute)?.value(),
        None => message.value()?,
    };
    Some(bundle.format_pattern(pattern, args, &mut vec![]).into_owned())
}

/// Retrieves a localized string based on the given identifier and optional attributes or arguments.
///
/// This function attempts to fetch a translation string from the context's available
/// translation resources for the current locale. If the locale-specific translation is not found,
/// it falls back to a main/default translation resource. If neither is available, it logs a warning
/// and returns the fallback value of the identifier itself.
///
/// # Arguments
///
/// * `ctx` - The [`Context`] object containing necessary resources and configurations for localization.
/// * `id` - A string slice identifying the translation message.
/// * `attr` - An optional attribute to retrieve a specific variant of the translation (can be `None`).
/// * `args` - An optional set of arguments of type `FluentArgs` to interpolate into the message (can be `None`).
///
/// # Returns
///
/// Returns the formatted localized string. If no translation is found, the identifier itself is returned as a fallback.
///
/// # Behavior
///
/// 1. Fetches the translation resource based on the current locale from `ctx.data().translations`.
/// 2. Attempts to format the string using `translations.other` for the given locale.
/// 3. Falls back to a global/main translation resource if the locale-specific resource is not found.
/// 4. Logs a warning if the translation is missing and uses the `id` as the fallback value.
///
/// # Example
///
/// ```rust
/// let message = get(ctx, "welcome_message", None, None);
/// println!("{}", message);
/// ```
///
/// In this example, the function attempts to retrieve the `welcome_message` string based on
/// the current locale and outputs it to the console. If unavailable, it will log a warning
/// and print `"welcome_message"` as the fallback.
///
/// # Errors
///
/// * Logs a warning using `tracing` if no translation message is found with the provided `id` and locale.
///
/// # Dependencies
///
/// This function relies on:
/// * A properly configured [`Context`] object providing locale and translation data.
/// * Fluent localization features for interpolation and message formatting.
///
/// [`Context`]: poise::Context
#[allow(unused)]
pub fn get(
    ctx: Context,
    id: &str,
    attr: Option<&str>,
    args: Option<&FluentArgs<'_>>,
) -> String {
    let translations = &ctx.data().translations;
    ctx.locale()
        .and_then(|locale| format(translations.other.get(locale)?, id, attr, args))
        .or_else(|| format(&translations.main, id, attr, args))
        .unwrap_or_else(|| {
            tracing::warn!("Unknown Fluent message identifier `{}`", id);
            id.to_string()
        })
}

/// Retrieves a localized string based on the given identifier, locale, and optional attributes or arguments.
///
/// This function attempts to fetch a translation string from the provided translations for a specific
/// locale string (e.g., "en-US"). If the locale-specific translation is not found, it falls back to
/// the main/default translation resource. If neither is available, it returns the identifier itself.
///
/// # Arguments
///
/// * `translations` - A reference to the `Translations` struct containing translation bundles.
/// * `locale` - A string slice identifying the locale (e.g., "en-US", "fr", "es").
/// * `id` - A string slice identifying the translation message.
/// * `attr` - An optional attribute to retrieve a specific variant of the translation (can be `None`).
/// * `args` - An optional set of arguments of type `FluentArgs` to interpolate into the message (can be `None`).
///
/// # Returns
///
/// Returns the formatted localized string. If no translation is found, the identifier itself is returned as a fallback.
///
/// # Behavior
///
/// 1. Fetches the translation resource based on the provided locale from `translations.other`.
/// 2. Attempts to format the string using the locale-specific bundle.
/// 3. Falls back to the main translation resource if the locale-specific resource is not found.
/// 4. Returns the `id` as the fallback value if the translation is missing.
///
/// # Example
///
/// ```rust
/// let message = get_by_locale(&translations, "en-US", "welcome_message", None, None);
/// println!("{}", message);
/// ```
///
/// In this example, the function attempts to retrieve the `welcome_message` string for the "en-US"
/// locale and outputs it to the console. If unavailable, it will return `"welcome_message"` as the fallback.
pub fn get_by_locale(
    locale: &str,
    id: &str,
    attr: Option<&str>,
    args: Option<&FluentArgs<'_>>,
) -> String {
    TRANSLATIONS.other
        .get(locale)
        .and_then(|bundle| format(bundle, id, attr, args))
        .or_else(|| format(&TRANSLATIONS.main, id, attr, args))
        .unwrap_or_else(|| id.to_string())
}

/// Reads Fluent translation files from the "translations" directory and returns a `Translations` object.
///
/// # Description
/// This function processes Fluent `.ftl` files to create a `Translations` object, which contains:
/// - The main translations bundle (`main`) built from the `en-US.ftl` file.
/// - Any additional translation bundles (`other`) present in the "translations" directory.
///
/// Each `.ftl` file is expected to have a valid locale name as its filename (e.g., `en-US.ftl`).
///
/// # Return
/// Returns a `Result` which:
/// - On success, contains a `Translations` object with the loaded translation bundles.
/// - On failure, contains an `Error` describing what went wrong during the reading or parsing process.
///
/// # Errors
/// The function can fail for several reasons:
/// - Problems reading a `.ftl` file (e.g., file not found or permission issues).
/// - Invalid or unparsable `.ftl` file contents.
/// - Issues deriving localization settings from the filenames.
/// - Problems parsing locales or building the Fluent `Bundle`.
///
/// # Internal Helper Function
/// `read_single_ftl`:
///   - A helper function that reads a single `.ftl` file, parses its contents, and returns a tuple containing:
///     - The locale string (derived from the filename).
///     - An associated Fluent `Bundle` object.
///
/// # Examples
/// ```
/// use your_crate::read_ftl;
///
/// match read_ftl() {
///     Ok(translations) => {
///         println!("Main translation loaded successfully.");
///         println!("Other translations loaded: {}", translations.other.len());
///     },
///     Err(e) => eprintln!("Error loading translations: {:?}", e),
/// }
/// ```
///
/// # See Also
/// - `FluentResource`: Used for compiling Fluent translation strings.
/// - `Bundle`: Represents a collection of Fluent localization data.
///
/// # Dependencies
/// - The "translations" directory must be available and contain valid `.ftl` files.
/// - The `translations/en-US.ftl` file is expected to exist and serve as the main translation file.
///
/// # Arguments
/// None.
///
/// # Return Type
/// `Result<Translations, Error>`
/// - On success, contains the `Translations` object.
/// - On failure, an `Error` variant.
pub fn read_ftl() -> Result<Translations, Error> {
    fn read_single_ftl(path: &Path) -> Result<(String, Bundle), Error> {
        let locale = path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid .ftl filename")?;

        let file_contents = std::fs::read_to_string(path)?;
        let resource = FluentResource::try_new(file_contents)
            .map_err(|(_, e)| format!("Failed to parse {:?}: {:?}", path, e))?;

        let mut bundle = Bundle::new_concurrent(vec![locale.parse()?]);
        bundle.add_resource(resource)
            .map_err(|e| format!("Failed to add resource to bundle: {:?}", e))?;

        Ok((locale.to_string(), bundle))
    }

    Ok(Translations {
        main: read_single_ftl("translations/en-US.ftl".as_ref())?.1,
        other: std::fs::read_dir("translations")?
            .map(|entry| read_single_ftl(&entry?.path()))
            .collect::<Result<_, _>>()?,
    })
}

/// Updates the localization for commands and their subcommands.
///
/// This function modifies the command names, descriptions, parameters, and choices
/// based on the provided translations. It also recursively processes any subcommands
/// within the given commands.
///
/// # Arguments
///
/// * `translations` - A reference to a `Translations` struct that contains the main
///   and other translation bundles. Each bundle provides locale-specific localization strings.
/// * `commands` - A mutable reference to a slice of `poise::Command` items. Each command
///   is updated to include localized names, descriptions, and parameter data based on the
///   translation bundles provided.
///
/// # Behavior
///
/// 1. Iterates over the `commands` slice and applies translations using the locale-specific
///    bundles in the `translations.other` field.
/// 2. Updates:
///    - The `name` and `description` of the command.
///    - The `name` and `description` of each parameter in the command.
///    - Each parameter's choice names.
/// 3. Falls back to the `translations.main` bundle for default localization if a specific
///    locale is not explicitly provided.
/// 4. Recursively invokes itself on any subcommands defined under each command.
///
/// # Localization Logic
///
/// - For each `locale` in `translations.other`, the function:
///   - Localizes the command name and description based on the bundle.
///   - Localizes each parameter's name and description.
///   - Localizes the names of parameter choices.
/// - For the main (default) translation bundle:
///   - Updates the primary `name` and `description` of the command.
///   - Updates the primary `name` and `description` of each parameter.
///   - Updates the primary name of each choice in the parameters.
///
/// # Example
///
/// ```rust
/// let translations = Translations {
///     main: Bundle { /* main translation bundle */ },
///     other: HashMap::from([
///         ("es".into(), Bundle { /* Spanish translation bundle */ }),
///         ("fr".into(), Bundle { /* French translation bundle */ }),
///     ]),
/// };
///
/// let mut commands = vec![/* some poise::Command values */];
/// apply_translations(&translations, &mut commands);
/// ```
///
/// After executing the function, the `commands` slice will have all the names, descriptions,
/// parameters, and subcommands updated as per the localization definitions.
pub fn apply_translations(
    translations: &Translations,
    commands: &mut [poise::Command<Data, Error>],
) {
    for command in commands {
        let original_name = command.name.clone();

        for (locale, bundle) in &translations.other {
            if let Some(name) = format(bundle, &original_name, None, None) {
                command.name_localizations.insert(locale.clone(), name);
                if let Some(desc) = format(bundle, &original_name, Some("description"), None) {
                    command.description_localizations.insert(locale.clone(), desc);
                }

                for param in &mut command.parameters {
                    if let Some(p_name) = format(bundle, &original_name, Some(&param.name), None) {
                        param.name_localizations.insert(locale.clone(), p_name);
                    }
                    if let Some(p_desc) =
                        format(bundle, &original_name, Some(&format!("{}-description", param.name)), None)
                    {
                        param.description_localizations.insert(locale.clone(), p_desc);
                    }
                    for choice in &mut param.choices {
                        if let Some(c_name) = format(bundle, &choice.name, None, None) {
                            choice.localizations.insert(locale.clone(), c_name);
                        }
                    }
                }
            }
        }

        // Fallback to main bundle
        let bundle = &translations.main;
        if let Some(name) = format(bundle, &original_name, None, None) {
            command.name = name;
            if let Some(desc) = format(bundle, &original_name, Some("description"), None) {
                command.description = Some(desc);
            }

            for param in &mut command.parameters {
                let original_param_name = param.name.clone();

                // IMPORTANT: do not overwrite param.name (internal option name)
                if let Some(p_name) = format(bundle, &original_name, Some(&original_param_name), None) {
                    param.name_localizations.insert("en-US".to_string(), p_name);
                }

                if let Some(p_desc) =
                    format(bundle, &original_name, Some(&format!("{}-description", original_param_name)), None)
                {
                    param.description = Some(p_desc);
                }

                // IMPORTANT: do not overwrite choice.name (internal choice key)
                for choice in &mut param.choices {
                    if let Some(c_name) = format(bundle, &choice.name, None, None) {
                        choice.localizations.insert("en-US".to_string(), c_name);
                    }
                }
            }
        }

        if !command.subcommands.is_empty() {
            apply_translations(translations, &mut command.subcommands);
        }
    }
}

/// Extracts variable names enclosed within `{$...}` placeholders from a given pattern string.
///
/// The function takes a string pattern and uses a regular expression to identify all occurrences
/// of variables enclosed within `{$...}`. These variables must follow the format of a `$`
/// immediately followed by one or more word characters (letters, digits, or underscores).
///
/// # Arguments
///
/// * `pattern` - A string slice containing the text pattern to search for variables.
///
/// # Returns
///
/// A vector of strings containing the names of all variables found in the pattern.
/// If no variables are found, the vector will be empty.
///
/// # Example
///
/// ```rust
/// let pattern = "Hello, {$name}. Welcome to {$location}.";
/// let variables = extract_variables_from_pattern(pattern);
/// assert_eq!(variables, vec!["name", "location"]);
/// ```
///
/// # Panics
///
/// This function panics if the regular expression fails to compile. However, the regex used
/// in this function (`r"\{\$(\w+)\}"`) is hardcoded and should always compile successfully.
fn extract_variables_from_pattern(pattern: &str) -> Vec<String> {
    Regex::new(r"\{\$(\w+)\}")
        .unwrap()
        .captures_iter(pattern)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// This function retrieves a localized translation for a given message `id` based on the current context, locale,
/// and any explicit arguments provided.
///
/// # Arguments
///
/// * `ctx` - The `Context` containing translation data and locale information.
/// * `id` - The identifier of the translation token to be retrieved.
/// * `explicit_args` - Optional arguments (`FluentArgs`) that may be explicitly provided for substitution
///   in the translation string.
///
/// # Returns
///
/// * `Result<String, Error>` - If successful,
pub fn smart_tr(
    ctx: Context,
    id: &str,
    explicit_args: Option<&FluentArgs>,
) -> Result<String, Error> {
    let translations = &ctx.data().translations;
    let bundle = ctx.locale()
        .and_then(|locale| translations.other.get(locale))
        .unwrap_or(&translations.main);

    let (base_id, attr) = if id.contains('.') {
        let parts: Vec<&str> = id.splitn(2, '.').collect();
        (parts[0], Some(parts[1]))
    } else {
        (id, None)
    };

    // If the token doesn't exist, just return it (visible + debuggable).
    let message = match bundle.get_message(base_id).or_else(|| translations.main.get_message(base_id)) {
        Some(message) => message,
        None => return Ok(id.to_string()),
    };

    // If the message exists but has no value, also fall back to the token.
    let pattern = match attr {
        Some(a) => match message.get_attribute(a) {
            Some(attr) => attr.value(),
            None => return Ok(id.to_string()),
        },
        None => match message.value() {
            Some(pattern) => pattern,
            None => return Ok(id.to_string()),
        },
    };

    let raw_text = bundle.format_pattern(pattern, None, &mut vec![]).into_owned();
    let used_vars = extract_variables_from_pattern(&raw_text);

    let mut args = FluentArgs::new();
    if let Some(explicit) = explicit_args {
        for (k, v) in explicit.iter() {
            args.set(k, v.clone());
        }
    }

    if args.get("support_link").is_none() {
        if let Some(link) = format(bundle, "support_link", None, None)
            .or_else(|| format(&translations.main, "support_link", None, None))
        {
            args.set("support_link", FluentValue::from(link));
        }
    }

    for var in used_vars {
        if args.get(&var).is_none() {
            let fallback_id = var.clone();
            if let Some(value) = format(bundle, &fallback_id, None, None)
                .or_else(|| format(&translations.main, &fallback_id, None, None))
            {
                args.set(var.clone(), FluentValue::from(value));
            } else {
                // Can't resolve a required variable -> return the token
                // (alternatively, return `raw_text` to show `{$var}` placeholders).
                return Ok(id.to_string());
            }
        }
    }

    Ok(bundle.format_pattern(pattern, Some(&args), &mut vec![]).into_owned())
}