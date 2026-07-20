//! [`UpdateSku`].

use serde::Deserialize;

use super::{SkuComponent, SkuKind};

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSku {
    pub sku_code: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub kind: SkuKind,
    pub active: bool,
    #[serde(default)]
    pub components: Vec<SkuComponent>,
}
