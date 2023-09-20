use config::{Config, ConfigError, Environment, File};
use directories::ProjectDirs;
use log::info;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct SurferConfig {
    pub layout: SurferLayout,
}

#[derive(Debug, Deserialize)]
pub struct SurferLayout {
    /// Flag to show/hide the hierarchy view
    pub show_hierarchy: bool,
}

impl SurferConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let mut c = Config::builder().set_default("layout.show_hierarchy", true)?;

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
