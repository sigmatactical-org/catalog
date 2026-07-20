//! [`SkuForm`].

use super::{CreateSku, SkuComponent, SkuKind, UpdateSku, empty_to_none};

#[derive(Debug, Clone)]
pub struct SkuForm {
    pub sku_code: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub kind: String,
    pub active: bool,
    /// Selected components as submitted: `(sku_id, quantity string)`.
    pub components: Vec<(String, String)>,
}
impl SkuForm {
    /// Build from raw urlencoded form pairs. Components come from repeated
    /// `component=<sku_id>` checkboxes paired with `qty_<sku_id>` inputs.
    #[must_use]
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let field = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        let components = pairs
            .iter()
            .filter(|(key, _)| key == "component")
            .map(|(_, sku_id)| (sku_id.clone(), field(&format!("qty_{sku_id}"))))
            .collect();
        Self {
            sku_code: field("sku_code"),
            name: field("name"),
            description: field("description"),
            category: field("category"),
            kind: field("kind"),
            active: pairs.iter().any(|(key, _)| key == "active"),
            components,
        }
    }

    /// Validate the form into a create request.
    pub fn into_create(self) -> Result<CreateSku, String> {
        let kind: SkuKind = self.kind.parse()?;
        let components = if kind == SkuKind::Composite {
            self.parsed_components()?
        } else {
            Vec::new()
        };
        Ok(CreateSku {
            sku_code: self.sku_code,
            name: self.name,
            description: empty_to_none(self.description),
            category: empty_to_none(self.category),
            kind,
            active: Some(self.active),
            components,
        })
    }

    /// Validate the form into an update request.
    pub fn into_update(self) -> Result<UpdateSku, String> {
        let kind: SkuKind = self.kind.parse()?;
        let components = if kind == SkuKind::Composite {
            self.parsed_components()?
        } else {
            Vec::new()
        };
        Ok(UpdateSku {
            sku_code: self.sku_code,
            name: self.name,
            description: empty_to_none(self.description),
            category: empty_to_none(self.category),
            kind,
            active: self.active,
            components,
        })
    }

    fn parsed_components(&self) -> Result<Vec<SkuComponent>, String> {
        self.components
            .iter()
            .map(|(sku_id, qty)| {
                let quantity = parse_quantity(qty)
                    .ok_or_else(|| format!("invalid quantity for component {sku_id}"))?;
                Ok(SkuComponent {
                    sku_id: sku_id.clone(),
                    quantity,
                })
            })
            .collect()
    }

    /// Selected components with bad quantities coerced to 1 — only for
    /// refilling the form after a validation error, never for persisting.
    #[must_use]
    pub fn components_lenient(&self) -> Vec<SkuComponent> {
        self.components
            .iter()
            .map(|(sku_id, qty)| SkuComponent {
                sku_id: sku_id.clone(),
                quantity: parse_quantity(qty).unwrap_or(1),
            })
            .collect()
    }
}

/// Parse a quantity field: empty means 1 (checkbox without its qty input).
fn parse_quantity(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(1);
    }
    trimmed.parse().ok().filter(|quantity| *quantity >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn from_pairs_collects_checked_components_with_quantities() {
        let form = SkuForm::from_pairs(&pairs(&[
            ("sku_code", "KIT-01"),
            ("name", "Starter kit"),
            ("description", ""),
            ("category", ""),
            ("kind", "composite"),
            ("component", "id-a"),
            ("component", "id-b"),
            ("qty_id-a", "2"),
            ("qty_id-b", "1"),
            ("qty_id-unchecked", "5"),
        ]));
        assert_eq!(
            form.components,
            vec![
                ("id-a".to_string(), "2".to_string()),
                ("id-b".to_string(), "1".to_string()),
            ]
        );
        let create = form.into_create().unwrap();
        assert_eq!(create.components.len(), 2);
        assert_eq!(create.components[0].quantity, 2);
    }

    #[test]
    fn into_create_rejects_invalid_quantity() {
        let form = SkuForm::from_pairs(&pairs(&[
            ("sku_code", "KIT-01"),
            ("name", "Starter kit"),
            ("kind", "composite"),
            ("component", "id-a"),
            ("qty_id-a", "0"),
        ]));
        let err = form.into_create().unwrap_err();
        assert!(err.contains("id-a"));
    }

    #[test]
    fn simple_kind_drops_selected_components() {
        let form = SkuForm::from_pairs(&pairs(&[
            ("sku_code", "WIDGET-01"),
            ("name", "Widget"),
            ("kind", "simple"),
            ("active", "on"),
            ("component", "id-a"),
        ]));
        let create = form.into_create().unwrap();
        assert!(create.components.is_empty());
        assert_eq!(create.active, Some(true));
    }
}
