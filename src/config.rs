use std::path::PathBuf;

/// Path to the JSON catalog database.
#[must_use]
pub fn data_path() -> PathBuf {
    std::env::var("CATALOG_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/catalog.json"))
}
