//! [`SkuRow`].

use crate::model::Sku;

use super::ComponentLine;

/// One rendered table row.
pub struct SkuRow {
    pub sku: Sku,
    pub kind_label: &'static str,
    pub updated_at: String,
    pub components: Vec<ComponentLine>,
}
