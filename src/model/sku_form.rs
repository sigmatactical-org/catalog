//! [`SkuForm`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SkuForm {
    pub sku_code: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub kind: String,
    pub active: Option<String>,
    pub components: String,
}
impl SkuForm {
    /// Validate the form into a create request.
    pub fn into_create(self) -> Result<CreateSku, String> {
        let kind = parse_kind(&self.kind)?;
        let components = if kind == SkuKind::Composite {
            parse_components_text(&self.components)?
        } else {
            Vec::new()
        };
        Ok(CreateSku {
            sku_code: self.sku_code,
            name: self.name,
            description: empty_to_none(self.description),
            category: empty_to_none(self.category),
            kind,
            active: Some(self.active.is_some()),
            components,
        })
    }

    /// Validate the form into an update request.
    pub fn into_update(self) -> Result<UpdateSku, String> {
        let kind = parse_kind(&self.kind)?;
        let components = if kind == SkuKind::Composite {
            parse_components_text(&self.components)?
        } else {
            Vec::new()
        };
        Ok(UpdateSku {
            sku_code: self.sku_code,
            name: self.name,
            description: empty_to_none(self.description),
            category: empty_to_none(self.category),
            kind,
            active: self.active.is_some(),
            components,
        })
    }
}
