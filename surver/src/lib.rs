//! External access to the Surver server.
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
pub use server::server_main;

pub const HTTP_SERVER_KEY: &str = "Server";
pub const HTTP_SERVER_VALUE_SURFER: &str = "Surfer";
pub const X_WELLEN_VERSION: &str = "x-wellen-version";
pub const X_SURFER_VERSION: &str = "x-surfer-version";
pub const SURFER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WELLEN_VERSION: &str = wellen::VERSION;

pub const WELLEN_SURFER_DEFAULT_OPTIONS: wellen::LoadOptions = wellen::LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: true,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    pub bytes: u64,
    pub bytes_loaded: u64,
    pub filename: String,
    pub wellen_version: String,
    pub surfer_version: String,
    pub file_format: wellen::FileFormat,
}

lazy_static! {
    pub static ref BINCODE_OPTIONS: bincode::DefaultOptions = bincode::DefaultOptions::new();
}
