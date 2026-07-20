//! [`Sku`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CreateSku, SkuComponent, SkuKind, UpdateSku};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sku {
    pub id: String,
    pub sku_code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub kind: SkuKind,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<SkuComponent>,
    pub updated_at: DateTime<Utc>,
}
impl Sku {
    /// New Sku from a create request.
    pub fn new(input: CreateSku) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            sku_code: input.sku_code.trim().to_string(),
            name: input.name.trim().to_string(),
            description: input.description.map(|s| s.trim().to_string()),
            category: input.category.map(|s| s.trim().to_string()),
            kind: input.kind,
            active: input.active.unwrap_or(true),
            components: input.components,
            updated_at: Utc::now(),
        }
    }

    /// Apply a partial update in place.
    pub fn apply_update(&mut self, input: UpdateSku) {
        self.sku_code = input.sku_code.trim().to_string();
        self.name = input.name.trim().to_string();
        self.description = input.description.map(|s| s.trim().to_string());
        self.category = input.category.map(|s| s.trim().to_string());
        self.kind = input.kind;
        self.active = input.active;
        self.components = input.components;
        self.updated_at = Utc::now();
    }
}
