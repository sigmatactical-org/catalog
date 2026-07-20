//! [`ComponentOption`].

/// One selectable component row in the SKU form picker.
pub struct ComponentOption {
    pub id: String,
    pub sku_code: String,
    pub name: String,
    pub selected: bool,
    pub quantity: u32,
}
