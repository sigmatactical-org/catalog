//! [`ComponentLine`].

#[allow(unused_imports)]
use super::*;

/// One resolved component shown in the index component list.
pub struct ComponentLine {
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
}
