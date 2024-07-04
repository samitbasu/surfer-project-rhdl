mod client;

use serde::{Deserialize, Serialize};

pub use client::{get_hierarchy, get_signals, get_status, get_time_table};

#[derive(Serialize, Deserialize)]
pub struct HierarchyResponse {
    pub hierarchy: wellen::Hierarchy,
    pub file_format: wellen::FileFormat,
}
