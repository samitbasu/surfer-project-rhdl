use color_eyre::eyre::{Context, Result};
use color_eyre::Report;
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
#[cfg(not(target_arch = "wasm32"))]
use config::{Environment, File};
use derive_more::Display;
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
use ecolor::Color32;
use enum_iterator::Sequence;
use serde::de;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::time::TimeFormat;
use crate::{clock_highlighting::ClockHighlightType, variable_name_type::VariableNameType};

#[derive(Debug, Deserialize, Display, PartialEq, Eq, Sequence)]
pub enum HierarchyStyle {
    #[display("Separate")]
    Separate,

    #[display("Tree")]
    Tree,
}

impl From<String> for HierarchyStyle {
    fn from(string: String) -> Self {
        match string.as_str() {
            "Separate" => Self::Separate,
            "Tree" => Self::Tree,
            _ => Self::Separate,
        }
    }
}

/// Selects the function of the arrow keys
#[derive(Debug, Deserialize, Display, PartialEq, Eq, Sequence)]
pub enum ArrowKeyBindings {
    /// The left/right arrow keys step to the next edge
    #[display("Edge")]
    Edge,

    /// The left/right arrow keys scroll the viewport left/right
    #[display("Scroll")]
    Scroll,
}

impl From<String> for ArrowKeyBindings {
    fn from(string: String) -> Self {
        match string.as_str() {
            "Edge" => Self::Edge,
            "Scroll" => Self::Scroll,
            _ => Self::Edge,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferConfig {
    pub layout: SurferLayout,
    #[serde(deserialize_with = "deserialize_theme")]
    pub theme: SurferTheme,
    pub gesture: SurferGesture,
    pub behavior: SurferBehavior,
    /// Time stamp format
    pub default_time_format: TimeFormat,
    // #[serde(deserialize_with = "deserialize_variable_name_type")]
    pub default_variable_name_type: VariableNameType,
    pub default_clock_highlight_type: ClockHighlightType,
    /// Distance in pixels for cursor snap
    pub snap_distance: f32,
    /// Maximum size of the undo stack
    pub undo_stack_size: usize,
    /// Some(true) - always auto reload changed files
    /// Some(false) - never auto reload changed files
    /// None - ask for confirmation before auto reloading files
    pub autoreload_files: Option<bool>,
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
    /// Flag to show/hide tooltip for variables and scopes
    show_tooltip: bool,
    /// Flag to show/hide the overview
    show_overview: bool,
    /// Flag to show/hide the statusbar
    pub show_statusbar: bool,
    /// Flag to show/hide the indices of variables in the variable list
    show_variable_indices: bool,
    /// Flag to show/hide the variable direction icon
    show_variable_direction: bool,
    /// Flag to show/hide empty scopes
    show_empty_scopes: bool,
    /// Flag to show parameters in scope view
    show_parameters_in_scopes: bool,
    /// Initial window height
    pub window_height: usize,
    /// Initial window width
    pub window_width: usize,
    /// Align variable names right
    align_names_right: bool,
    /// Set style of hierarchy
    pub hierarchy_style: HierarchyStyle,
    /// Text size in points for values in waves
    pub waveforms_text_size: f32,
    /// Line height in points for waves
    pub waveforms_line_height: f32,
    /// Lline height in points for transaction streams
    pub transactions_line_height: f32,
    /// UI zoom factors
    pub zoom_factors: Vec<f32>,
    /// Default UI zoom factor
    pub default_zoom_factor: f32,
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
    pub fn show_tooltip(&self) -> bool {
        self.show_tooltip
    }
    pub fn show_toolbar(&self) -> bool {
        self.show_toolbar
    }
    pub fn show_overview(&self) -> bool {
        self.show_overview
    }
    pub fn show_statusbar(&self) -> bool {
        self.show_statusbar
    }
    pub fn align_names_right(&self) -> bool {
        self.align_names_right
    }
    pub fn show_variable_indices(&self) -> bool {
        self.show_variable_indices
    }
    pub fn show_variable_direction(&self) -> bool {
        self.show_variable_direction
    }
    pub fn default_zoom_factor(&self) -> f32 {
        self.default_zoom_factor
    }
    pub fn show_empty_scopes(&self) -> bool {
        self.show_empty_scopes
    }
    pub fn show_parameters_in_scopes(&self) -> bool {
        self.show_parameters_in_scopes
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferBehavior {
    /// Keep or remove variables if unavailable during reload
    pub keep_during_reload: bool,
    /// Select the functionality bound to the arrow keys
    pub arrow_key_bindings: ArrowKeyBindings,
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
    /// Color used for text across the markers
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub alt_text_color: Color32,
    /// Colors used for the background and text of the wave view
    pub canvas_colors: ThemeColorTriple,
    /// Colors used for most UI elements not on the variable canvas
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
    /// Draw arrows on rising clock edges
    pub clock_rising_marker: bool,

    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Default variable color
    pub variable_default: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for high-impedance variables
    pub variable_highimp: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for undefined variables
    pub variable_undef: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for don't-care variables
    pub variable_dontcare: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for weak variables
    pub variable_weak: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for constant variables (parameters)
    pub variable_parameter: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Default transaction color
    pub transaction_default: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    // Color used for highlighting selected transaction
    pub transaction_highlight: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    // Color used for relation arrows of transactions
    pub relation_arrow: Color32,

    /// Opacity with which variable backgrounds are drawn. 0 is fully transparent and 1 is fully
    /// opaque.
    pub waveform_opacity: f32,

    #[serde(default = "default_colors", deserialize_with = "deserialize_color_map")]
    pub colors: HashMap<String, Color32>,

    /// variable line width
    pub linewidth: f32,

    /// Number of lines using standard background before changing to
    /// alternate background and so on, set to zero to disable
    pub alt_frequency: usize,

    /// Viewport separator line
    pub viewport_separator: SurferLineStyle,

    // drag hint and threshold parameters
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub drag_hint_color: Color32,
    pub drag_hint_width: f32,
    pub drag_threshold: f32,

    /// Tick information
    pub ticks: SurferTicks,

    /// List of theme names
    #[serde(default = "Vec::new")]
    pub theme_names: Vec<String>,
}

fn get_luminance(color: &Color32) -> f32 {
    let rg = if color.r() < 10 {
        color.r() as f32 / 3294.0
    } else {
        (color.r() as f32 / 269.0 + 0.0513).powf(2.4)
    };
    let gg = if color.g() < 10 {
        color.g() as f32 / 3294.0
    } else {
        (color.g() as f32 / 269.0 + 0.0513).powf(2.4)
    };
    let bg = if color.b() < 10 {
        color.b() as f32 / 3294.0
    } else {
        (color.b() as f32 / 269.0 + 0.0513).powf(2.4)
    };
    0.2126 * rg + 0.7152 * gg + 0.0722 * bg
}

impl SurferTheme {
    pub fn get_color(&self, color: &String) -> Option<&Color32> {
        let case_sensitive = self.colors.get(color);
        if case_sensitive.is_some() {
            return case_sensitive;
        }
        // Hack to handle issue with config 0.14
        let case_insensitive = self.colors.get(&color.to_lowercase());
        if case_insensitive.is_some() {
            return case_insensitive;
        }
        None
    }

    pub fn get_best_text_color(&self, backgroundcolor: &Color32) -> &Color32 {
        // Based on https://ux.stackexchange.com/questions/82056/how-to-measure-the-contrast-between-any-given-color-and-white

        // Compute luminance
        let l_foreground = get_luminance(&self.foreground);
        let l_alt_text_color = get_luminance(&self.alt_text_color);
        let l_background = get_luminance(backgroundcolor);

        // Compute contrast ratio
        let mut cr_foreground = (l_foreground + 0.05) / (l_background + 0.05);
        cr_foreground = cr_foreground.max(1. / cr_foreground);
        let mut cr_alt_text_color = (l_alt_text_color + 0.05) / (l_background + 0.05);
        cr_alt_text_color = cr_alt_text_color.max(1. / cr_alt_text_color);

        // Return color with highest contrast
        if cr_foreground > cr_alt_text_color {
            &self.foreground
        } else {
            &self.alt_text_color
        }
    }

    fn generate_defaults(
        theme_name: &Option<String>,
    ) -> (ConfigBuilder<DefaultState>, Vec<String>) {
        let default_theme = String::from(include_str!("../../default_theme.toml"));

        let mut theme = Config::builder().add_source(config::File::from_str(
            &default_theme,
            config::FileFormat::Toml,
        ));

        let theme_names = all_theme_names();

        let override_theme = match theme_name.clone().unwrap_or_default().as_str() {
            "dark+" => include_str!("../../themes/dark+.toml"),
            "dark-high-contrast" => include_str!("../../themes/dark-high-contrast.toml"),
            "ibm" => include_str!("../../themes/ibm.toml"),
            "light+" => include_str!("../../themes/light+.toml"),
            "light-high-contrast" => include_str!("../../themes/light-high-contrast.toml"),
            "okabe/ito" => include_str!("../../themes/okabe-ito.toml"),
            "solarized" => include_str!("../../themes/solarized.toml"),
            _ => "",
        }
        .to_string();

        theme = theme.add_source(config::File::from_str(
            &override_theme,
            config::FileFormat::Toml,
        ));
        (theme, theme_names)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(theme_name: Option<String>) -> Result<Self> {
        use color_eyre::eyre::anyhow;

        let (mut theme, theme_names) = Self::generate_defaults(&theme_name);

        let theme = theme.set_override("theme_names", all_theme_names())?;

        theme
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(theme_name: Option<String>) -> color_eyre::Result<Self> {
        use std::fs::ReadDir;

        use color_eyre::eyre::anyhow;

        let (mut theme, mut theme_names) = Self::generate_defaults(&theme_name);

        let mut add_themes_from_dir = |dir: ReadDir| {
            for theme in dir.flatten() {
                if let Ok(theme_path) = theme.file_name().into_string() {
                    if theme_path.ends_with(".toml") {
                        let fname = theme_path.strip_suffix(".toml").unwrap().to_string();
                        if !fname.is_empty() && !theme_names.contains(&fname) {
                            theme_names.push(fname);
                        }
                    }
                }
            }
        };

        // read themes from config directory
        if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
            let config_themes_dir = proj_dirs.config_dir().join("themes");
            if let Ok(config_themes_dir) = std::fs::read_dir(config_themes_dir) {
                add_themes_from_dir(config_themes_dir);
            }
        }

        // Read themes from local directories.
        let local_config_dirs = find_local_configs();

        // Add any existing themes from most top-level to most local. This allows overwriting of
        // higher-level theme settings with a local `.surfer` directory.
        local_config_dirs
            .iter()
            .filter_map(|p| std::fs::read_dir(p.join("themes")).ok())
            .for_each(add_themes_from_dir);

        if theme_name
            .clone()
            .is_some_and(|theme_name| !theme_name.is_empty())
        {
            let theme_path = Path::new("themes").join(theme_name.unwrap() + ".toml");

            // First filter out all the existing local themes and add them in the aforementioned
            // order.
            let local_themes: Vec<PathBuf> = local_config_dirs
                .iter()
                .map(|p| p.join(&theme_path))
                .filter(|p| p.exists())
                .collect();
            if !local_themes.is_empty() {
                theme = local_themes
                    .into_iter()
                    .fold(theme, |t, p| t.add_source(File::from(p).required(false)));
            } else {
                // If no local themes exist, search in the config directory.
                if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
                    let config_theme_path = proj_dirs.config_dir().join(theme_path);
                    if config_theme_path.exists() {
                        theme = theme.add_source(File::from(config_theme_path).required(false));
                    }
                }
            }
        }

        let theme = theme.set_override("theme_names", theme_names)?;

        theme
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse theme {e}"))
    }
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
    pub fn new(_force_default_config: bool) -> Result<Self> {
        Self::new_from_toml(&include_str!("../../default_config.toml"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(force_default_config: bool) -> color_eyre::Result<Self> {
        use color_eyre::eyre::anyhow;
        use log::warn;

        let default_config = String::from(include_str!("../../default_config.toml"));

        let mut config = Config::builder().add_source(config::File::from_str(
            &default_config,
            config::FileFormat::Toml,
        ));

        let config = if !force_default_config {
            if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
                let config_file = proj_dirs.config_dir().join("config.toml");
                config = config.add_source(File::from(config_file).required(false));
            }

            if Path::new("surfer.toml").exists() {
                warn!("Configuration in 'surfer.toml' is deprecated. Please move your configuration to '.surfer/config.toml'.");
            }

            // `surfer.toml` will not be searched for upward, as it is deprecated.
            config = config.add_source(File::from(Path::new("surfer.toml")).required(false));

            // Add configs from most top-level to most local. This allows overwriting of
            // higher-level settings with a local `.surfer` directory.
            find_local_configs()
                .into_iter()
                .fold(config, |c, p| {
                    c.add_source(File::from(p.join("config.toml")).required(false))
                })
                .add_source(Environment::with_prefix("surfer")) // Add environment finally
        } else {
            config
        };

        config
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }

    pub fn new_from_toml(config: &str) -> Result<Self> {
        Ok(toml::from_str(config)?)
    }
}

impl Default for SurferConfig {
    fn default() -> Self {
        Self::new(false).expect("Failed to load default config")
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
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        let g = u8::from_str_radix(&str[2..4], 16)
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        let b = u8::from_str_radix(&str[4..6], 16)
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        Ok(Color32::from_rgb(r, g, b))
    } else {
        color_eyre::Result::Err(Report::msg(format!("'{str}' is not a valid RGB hex color")))
    }
}

fn all_theme_names() -> Vec<String> {
    vec![
        "dark+".to_string(),
        "dark-high-contrast".to_string(),
        "ibm".to_string(),
        "light+".to_string(),
        "light-high-contrast".to_string(),
        "okabe/ito".to_string(),
        "solarized".to_string(),
    ]
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

fn deserialize_theme<'de, D>(deserializer: D) -> Result<SurferTheme, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    SurferTheme::new(Some(buf)).map_err(de::Error::custom)
}

/// Searches for `.surfer` directories upward from the current location until it reaches root.
/// Returns an empty vector in case the search fails in any way. If any `.surfer` directories
/// are found, they will be returned in a `Vec<PathBuf>` in a pre-order of most top-level to most
/// local. All plain files are ignored.
#[cfg(not(target_arch = "wasm32"))]
fn find_local_configs() -> Vec<PathBuf> {
    use crate::util::search_upward;
    match std::env::current_dir() {
        Ok(dir) => search_upward(dir, "/", ".surfer")
            .into_iter()
            .filter(|p| p.is_dir()) // Only keep directories and ignore plain files.
            .rev() // Reverse for pre-order traversal of directories.
            .collect(),
        Err(_) => vec![],
    }
}
