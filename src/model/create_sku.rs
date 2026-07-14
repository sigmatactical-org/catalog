//! [`CreateSku`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSku {
    pub sku_code: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub kind: SkuKind,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub components: Vec<SkuComponent>,
}
