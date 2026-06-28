use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkuKind {
    Simple,
    Composite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkuComponent {
    pub sku_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sku {
    pub id: String,
    pub sku_code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub kind: SkuKind,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<SkuComponent>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSku {
    pub sku_code: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub kind: SkuKind,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub components: Vec<SkuComponent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSku {
    pub sku_code: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub kind: SkuKind,
    pub active: bool,
    #[serde(default)]
    pub components: Vec<SkuComponent>,
}

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

pub fn format_components_text(components: &[SkuComponent]) -> String {
    components
        .iter()
        .map(|c| format!("{} {}", c.sku_id, c.quantity))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Sku {
    pub fn new(input: CreateSku) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            sku_code: input.sku_code.trim().to_string(),
            name: input.name.trim().to_string(),
            description: input.description.map(|s| s.trim().to_string()),
            category: input.category.map(|s| s.trim().to_string()),
            kind: input.kind,
            active: input.active.unwrap_or(true),
            components: input.components,
            updated_at: now,
        }
    }

    pub fn apply_update(&mut self, input: UpdateSku) {
        self.sku_code = input.sku_code.trim().to_string();
        self.name = input.name.trim().to_string();
        self.description = input.description.map(|s| s.trim().to_string());
        self.category = input.category.map(|s| s.trim().to_string());
        self.kind = input.kind;
        self.active = input.active;
        self.components = input.components;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
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
