//! [`SkuRow`].

#[allow(unused_imports)]
use super::*;
use crate::model::Sku;

/// One rendered table row.
pub struct SkuRow {
    pub sku: Sku,
    pub kind_label: String,
    pub components: Vec<ComponentLine>,
}
