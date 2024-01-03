use color_eyre::eyre::{Context, Result};
use color_eyre::Report;
#[cfg(not(target_arch = "wasm32"))]
use config::{Config, Environment, File};
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
use eframe::epaint::Color32;
use serde::de;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use crate::time::TimeFormat;
use crate::{clock_highlighting::ClockHighlightType, signal_name_type::SignalNameType};

#[derive(Debug, Deserialize)]
pub struct SurferConfig {
    pub layout: SurferLayout,
    pub theme: SurferTheme,
    pub gesture: SurferGesture,
    /// Tick information
    pub ticks: SurferTicks,
    /// Time stamp format
    pub default_time_format: TimeFormat,
    // #[serde(deserialize_with = "deserialize_signal_name_type")]
    pub default_signal_name_type: SignalNameType,
    pub default_clock_highlight_type: ClockHighlightType,
}

#[derive(Debug, Deserialize)]
pub struct SurferLayout {
    /// Flag to show/hide the hierarchy view
    show_hierarchy: bool,
    /// Flag to show/hide the menu
    show_menu: bool,
    /// Flag to show/hide toolbar
    show_toolbar: bool,
    /// Flag to show/hide tick lines
    show_ticks: bool,
    /// Flag to show/hide tooltip for signals
    show_signal_tooltip: bool,
    /// Flag to show/hide the overview
    show_overview: bool,
    /// Initial window height
    pub window_height: usize,
    /// Initial window width
    pub window_width: usize,
}

impl SurferLayout {
    pub fn show_hierarchy(&self) -> bool {
        self.show_hierarchy
    }
    pub fn show_menu(&self) -> bool {
        self.show_menu
    }
    pub fn show_ticks(&self) -> bool {
        self.show_ticks
    }
    pub fn show_signal_tooltip(&self) -> bool {
        self.show_signal_tooltip
    }
    pub fn show_toolbar(&self) -> bool {
        self.show_toolbar
    }
    pub fn show_overview(&self) -> bool {
        self.show_overview
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferGesture {
    /// Line style for gesture lines
    pub style: SurferLineStyle,
    /// Size of the overlay help
    pub size: f32,
    /// (Squared) minimum distance to move to remove the overlay help and perform gesture
    pub deadzone: f32,
}

#[derive(Debug, Deserialize)]
pub struct SurferLineStyle {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub color: Color32,
    pub width: f32,
}

#[derive(Debug, Deserialize)]
pub struct SurferTicks {
    /// 0 to 1, where 1 means as many ticks that can fit without overlap
    pub density: f32,
    pub style: SurferLineStyle,
}

#[derive(Debug, Deserialize)]
pub struct SurferTheme {
    /// Color used for text across the UI
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color of borders between UI elements
    pub border_color: Color32,
    /// Colors used for the background and text of the wave view
    pub canvas_colors: ThemeColorTriple,
    /// Colors used for most UI elements not on the signal canvas
    pub primary_ui_color: ThemeColorPair,
    /// Colors used for the variable and value list, as well as secondary elements
    /// like text fields
    pub secondary_ui_color: ThemeColorPair,
    /// Color used for selected ui elements such as the currently selected hierarchy
    pub selected_elements_colors: ThemeColorPair,

    pub accent_info: ThemeColorPair,
    pub accent_warn: ThemeColorPair,
    pub accent_error: ThemeColorPair,

    ///  Line style for cursor
    pub cursor: SurferLineStyle,

    ///  Line style for clock highlight lines
    pub clock_highlight_line: SurferLineStyle,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub clock_highlight_cycle: Color32,

    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Default signal color
    pub signal_default: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for high-impedance signals
    pub signal_highimp: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for undefined signals
    pub signal_undef: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for don't-care signals
    pub signal_dontcare: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for weak signals
    pub signal_weak: Color32,
    #[serde(default = "default_colors", deserialize_with = "deserialize_color_map")]
    pub colors: HashMap<String, Color32>,

    /// Signal line width
    pub linewidth: f32,

    /// Number of lines using standard background before changing to
    /// alternate background and so on, set to zero to disable
    pub alt_frequency: usize,
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

#[derive(Debug, Deserialize)]
pub struct ThemeColorTriple {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub alt_background: Color32,
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
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Result<Self> {
        let default_config = String::from(include_str!("../default_config.toml"));
        Ok(toml::from_str(&default_config)?)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> color_eyre::Result<Self> {
        use color_eyre::eyre::anyhow;

        let default_config = String::from(include_str!("../default_config.toml"));

        let mut c = Config::builder().add_source(config::File::from_str(
            &default_config,
            config::FileFormat::Toml,
        ));

        if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
            let config_file = proj_dirs.config_dir().join("config.toml");
            c = c.add_source(File::from(config_file).required(false));
        }

        c.add_source(File::from(Path::new("surfer.toml")).required(false))
            .add_source(Environment::with_prefix("surfer"))
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }
}

impl Default for SurferConfig {
    fn default() -> Self {
        Self::new().expect("Failed to load default config")
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
