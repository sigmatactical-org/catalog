mod create_sku;
mod sku;
mod sku_component;
mod sku_form;
mod sku_kind;
mod update_sku;
pub use create_sku::CreateSku;
pub use sku::Sku;
pub use sku_component::SkuComponent;
pub use sku_form::SkuForm;
pub use sku_kind::SkuKind;
pub use update_sku::UpdateSku;

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
