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

fn parse_kind(value: &str) -> Result<SkuKind, String> {
    match value.trim().to_lowercase().as_str() {
        "simple" => Ok(SkuKind::Simple),
        "composite" => Ok(SkuKind::Composite),
        other => Err(format!("invalid kind: {other}")),
    }
}

/// Parse component lines as `<sku_id> <quantity>` (whitespace-separated).
pub fn parse_components_text(text: &str) -> Result<Vec<SkuComponent>, String> {
    let mut components = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let sku_id = parts
            .next()
            .ok_or_else(|| format!("line {}: missing sku id", line_no + 1))?
            .to_string();
        let qty_str = parts
            .next()
            .ok_or_else(|| format!("line {}: missing quantity", line_no + 1))?;
        if parts.next().is_some() {
            return Err(format!("line {}: too many fields", line_no + 1));
        }
        let quantity: u32 = qty_str
            .parse()
            .map_err(|_| format!("line {}: invalid quantity", line_no + 1))?;
        if quantity == 0 {
            return Err(format!("line {}: quantity must be at least 1", line_no + 1));
        }
        components.push(SkuComponent { sku_id, quantity });
    }
    Ok(components)
}

/// Render components as the multi-line `qty x sku` text form.
pub fn format_components_text(components: &[SkuComponent]) -> String {
    components
        .iter()
        .map(|c| format!("{} {}", c.sku_id, c.quantity))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_components_text_skips_comments_and_blank_lines() {
        let text = "# header\n\nabc123 2\n# another\ndef456 1\n";
        let components = parse_components_text(text).unwrap();
        assert_eq!(
            components,
            vec![
                SkuComponent {
                    sku_id: "abc123".to_string(),
                    quantity: 2,
                },
                SkuComponent {
                    sku_id: "def456".to_string(),
                    quantity: 1,
                },
            ]
        );
    }
}
