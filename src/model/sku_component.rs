//! [`SkuComponent`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkuComponent {
    pub sku_id: String,
    pub quantity: u32,
}
