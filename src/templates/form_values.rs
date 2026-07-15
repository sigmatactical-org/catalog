//! [`FormValues`].

#[allow(unused_imports)]
use super::*;
use crate::model::SkuComponent;

/// Prefilled field values for the edit/create form.
pub struct FormValues {
    pub sku_code: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub kind: String,
    pub active: bool,
    pub components: Vec<SkuComponent>,
}
