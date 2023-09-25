use color_eyre::eyre::{Context, Result};
use color_eyre::Report;
use config::{Config, ConfigError, Environment, File};
use directories::ProjectDirs;
use eframe::epaint::Color32;
use log::info;
use serde::de;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct SurferConfig {
    pub layout: SurferLayout,
    pub theme: SurferTheme,
}

#[derive(Debug, Deserialize)]
pub struct SurferLayout {
    /// Flag to show/hide the hierarchy view
    pub show_hierarchy: bool,
}

#[derive(Debug, Deserialize)]
pub struct SurferCursor {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub color: Color32,
    pub width: f32,
}

#[derive(Debug, Deserialize)]
pub struct SurferTheme {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    pub background1: ThemeColorPair,
    pub background2: ThemeColorPair,
    pub background3: ThemeColorPair,
    pub accent_info: ThemeColorPair,
    pub accent_warn: ThemeColorPair,
    pub accent_error: ThemeColorPair,

    pub cursor: SurferCursor,

    #[serde(deserialize_with = "deserialize_hex_color")]
    pub signal_default: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub signal_highimp: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub signal_undef: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub signal_dontcare: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub signal_weak: Color32,
    #[serde(default = "default_colors", deserialize_with = "deserialize_color_map")]
    pub colors: HashMap<String, Color32>,

    pub linewidth: f32,
}

#[derive(Debug, Deserialize)]
pub struct ThemeColor {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub color: Color32,
}

#[derive(Debug, Deserialize)]
pub struct ThemeColorPair {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: Color32,
}

fn default_colors() -> HashMap<String, Color32> {
    vec![
        ("Green", "a7e47e"),
        ("Red", "c52e2e"),
        ("Yellow", "f3d54a"),
        ("Blue", "81a2be"),
        ("Purple", "b294bb"),
        ("Aqua", "8abeb7"),
        ("Gray", "c5c8c6"),
    ]
    .iter()
    .map(|(name, hexcode)| {
        (
            name.to_string(),
            hex_string_to_color32(hexcode.to_string()).unwrap(),
        )
    })
    .collect()
}

impl SurferConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let fg_color = "ffffff".to_string();
        let bg1_color = "0b151d".to_string();
        let bg2_color = "0d1317".to_string();
        let bg3_color = "171717".to_string();
        let default_color = "8aea49".to_string();
        let undef_color = "dd1e1e".to_string();
        let highimp_color = "fad52c".to_string();
        let cursor_color = "ff8080".to_string();
        let dontcare_color = "4040ff".to_string();
        let weak_color = "808080".to_string();

        let mut c = Config::builder()
            .set_default("layout.show_hierarchy", true)?
            // Colors
            .set_default("theme.foreground", fg_color.clone())?
            .set_default("theme.background1.foreground", fg_color.clone())?
            .set_default("theme.background1.background", bg1_color.clone())?
            .set_default("theme.background2.foreground", fg_color.clone())?
            .set_default("theme.background2.background", bg2_color.clone())?
            .set_default("theme.background3.foreground", fg_color.clone())?
            .set_default("theme.background3.background", bg3_color.clone())?
            // cursor theme
            .set_default("theme.cursor.color", cursor_color.clone())?
            .set_default("theme.cursor.width", "3")?
            // signal colors
            .set_default("theme.signal_default", default_color.clone())?
            .set_default("theme.signal_undef", undef_color.clone())?
            .set_default("theme.signal_highimp", highimp_color.clone())?
            .set_default("theme.signal_dontcare", dontcare_color.clone())?
            .set_default("theme.signal_weak", weak_color.clone())?
            .set_default("theme.linewidth", "1")?
            // accent colors
            .set_default("theme.accent_error.foreground", bg2_color.clone())?
            .set_default("theme.accent_error.background", undef_color)?
            .set_default("theme.accent_warn.foreground", bg2_color.clone())?
            .set_default("theme.accent_warn.background", highimp_color)?
            .set_default("theme.accent_info.foreground", bg2_color.clone())?
            .set_default("theme.accent_info.background", default_color)?;

        if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
            let config_file = proj_dirs.config_dir().join("config.toml");
            info!("Add configuration from {:?}", config_file);
            c = c.add_source(File::from(config_file).required(false));
        }

        c.add_source(File::from(Path::new("surfer.toml")).required(false))
            .add_source(Environment::with_prefix("surfer"))
            .build()?
            .try_deserialize()
    }
}

fn hex_string_to_color32(mut str: String) -> Result<Color32> {
    let mut hex_str = String::new();
    if str.len() == 3 {
        for c in str.chars() {
            hex_str.push(c);
            hex_str.push(c);
        }
        str = hex_str;
    }
    if str.len() == 6 {
        let r = u8::from_str_radix(&str[0..2], 16)
            .with_context(|| format!("'{}' is not a valid RGB hex color", str))?;
        let g = u8::from_str_radix(&str[2..4], 16)
            .with_context(|| format!("'{}' is not a valid RGB hex color", str))?;
        let b = u8::from_str_radix(&str[4..6], 16)
            .with_context(|| format!("'{}' is not a valid RGB hex color", str))?;
        Ok(Color32::from_rgb(r, g, b))
    } else {
        color_eyre::Result::Err(Report::msg(format!(
            "'{}' is not a valid RGB hex color",
            str
        )))
    }
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<Color32, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    hex_string_to_color32(buf).map_err(de::Error::custom)
}

fn deserialize_color_map<'de, D>(deserializer: D) -> Result<HashMap<String, Color32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_hex_color")] Color32);

    let v = HashMap::<String, Wrapper>::deserialize(deserializer)?;
    Ok(v.into_iter().map(|(k, Wrapper(v))| (k, v)).collect())
}
