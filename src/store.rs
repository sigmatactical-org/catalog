use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sku not found")]
    NotFound,
    #[error("sku_code already exists")]
    DuplicateSkuCode,
    #[error("sku_code is required")]
    SkuCodeRequired,
    #[error("name is required")]
    NameRequired,
    #[error("composite sku must have at least one component")]
    CompositeNeedsComponents,
    #[error("simple sku cannot have components")]
    SimpleHasComponents,
    #[error("component sku not found: {0}")]
    ComponentNotFound(String),
    #[error("component quantity must be at least 1")]
    InvalidQuantity,
    #[error("composite sku cannot contain itself")]
    SelfReference,
    #[error("composite sku would create a cycle")]
    CycleDetected,
    #[error("sku is referenced by composite sku(s): {0}")]
    ReferencedByComposite(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Database {
    skus: Vec<Sku>,
}

#[derive(Debug, Clone)]
pub struct CatalogStore {
    path: PathBuf,
    db: Database,
}

impl CatalogStore {
    /// Load or initialize the catalog database at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let db = if path.exists() {
            let bytes = std::fs::read(&path)?;
            serde_json::from_slice(&bytes)?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Database::default()
        };
        Ok(Self { path, db })
    }

    fn save(&self) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.db)?;
        std::fs::write(&self.path, bytes)?;
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<Sku> {
        let mut skus = self.db.skus.clone();
        skus.sort_by(|a, b| a.sku_code.to_lowercase().cmp(&b.sku_code.to_lowercase()));
        skus
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<Sku> {
        self.db.skus.iter().find(|s| s.id == id).cloned()
    }

    pub fn create(&mut self, input: CreateSku) -> Result<Sku, StoreError> {
        self.validate_create(&input)?;
        let sku = Sku::new(input);
        self.db.skus.push(sku.clone());
        self.save()?;
        Ok(sku)
    }

    pub fn update(&mut self, id: &str, input: UpdateSku) -> Result<Sku, StoreError> {
        self.validate_update(id, &input)?;
        let sku = self
            .db
            .skus
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;
        sku.apply_update(input);
        let updated = sku.clone();
        self.save()?;
        Ok(updated)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), StoreError> {
        let index = self
            .db
            .skus
            .iter()
            .position(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;

        let referencers: Vec<String> = self
            .db
            .skus
            .iter()
            .filter(|s| s.kind == SkuKind::Composite)
            .filter(|s| s.components.iter().any(|c| c.sku_id == id))
            .map(|s| s.sku_code.clone())
            .collect();
        if !referencers.is_empty() {
            return Err(StoreError::ReferencedByComposite(referencers.join(", ")));
        }

        self.db.skus.remove(index);
        self.save()
    }

    fn validate_create(&self, input: &CreateSku) -> Result<(), StoreError> {
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, None) {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(None, input.kind, &input.components)
    }

    fn validate_update(&self, id: &str, input: &UpdateSku) -> Result<(), StoreError> {
        if self.get(id).is_none() {
            return Err(StoreError::NotFound);
        }
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, Some(id)) {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(Some(id), input.kind, &input.components)
    }

    fn validate_fields(
        &self,
        sku_code: &str,
        name: &str,
        kind: SkuKind,
        components: &[SkuComponent],
    ) -> Result<(), StoreError> {
        if sku_code.trim().is_empty() {
            return Err(StoreError::SkuCodeRequired);
        }
        if name.trim().is_empty() {
            return Err(StoreError::NameRequired);
        }
        match kind {
            SkuKind::Simple if !components.is_empty() => Err(StoreError::SimpleHasComponents),
            SkuKind::Composite if components.is_empty() => {
                Err(StoreError::CompositeNeedsComponents)
            }
            _ => Ok(()),
        }
    }

    fn sku_code_exists(&self, sku_code: &str, except_id: Option<&str>) -> bool {
        let normalized = sku_code.trim().to_lowercase();
        self.db
            .skus
            .iter()
            .any(|s| except_id != Some(s.id.as_str()) && s.sku_code.to_lowercase() == normalized)
    }

    fn validate_components(
        &self,
        self_id: Option<&str>,
        kind: SkuKind,
        components: &[SkuComponent],
    ) -> Result<(), StoreError> {
        if kind != SkuKind::Composite {
            return Ok(());
        }

        for component in components {
            if component.quantity == 0 {
                return Err(StoreError::InvalidQuantity);
            }
            if self_id == Some(component.sku_id.as_str()) {
                return Err(StoreError::SelfReference);
            }
            if self.get(&component.sku_id).is_none() {
                return Err(StoreError::ComponentNotFound(component.sku_id.clone()));
            }
        }

        if let Some(id) = self_id
            && self.would_cycle(id, components)?
        {
            return Err(StoreError::CycleDetected);
        }

        Ok(())
    }

    fn would_cycle(&self, root_id: &str, components: &[SkuComponent]) -> Result<bool, StoreError> {
        let graph = self.component_graph();
        let mut visited = HashSet::new();
        for component in components {
            if dfs_contains(&graph, &component.sku_id, root_id, &mut visited) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn component_graph(&self) -> HashMap<String, Vec<String>> {
        self.db
            .skus
            .iter()
            .filter(|s| s.kind == SkuKind::Composite)
            .map(|s| {
                (
                    s.id.clone(),
                    s.components.iter().map(|c| c.sku_id.clone()).collect(),
                )
            })
            .collect()
    }
}

fn dfs_contains(
    graph: &HashMap<String, Vec<String>>,
    current: &str,
    target: &str,
    visited: &mut HashSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    if !visited.insert(current.to_string()) {
        return false;
    }
    if let Some(children) = graph.get(current) {
        for child in children {
            if dfs_contains(graph, child, target, visited) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SkuComponent;
    use tempfile::TempDir;

    fn test_store() -> (CatalogStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = CatalogStore::load(dir.path().join("catalog.json")).unwrap();
        (store, dir)
    }

    #[test]
    fn create_simple_sku() {
        let (mut store, _dir) = test_store();
        let sku = store
            .create(CreateSku {
                sku_code: "WIDGET-01".to_string(),
                name: "Widget".to_string(),
                description: None,
                category: Some("parts".to_string()),
                kind: SkuKind::Simple,
                active: Some(true),
                components: vec![],
            })
            .unwrap();
        assert_eq!(sku.sku_code, "WIDGET-01");
        assert_eq!(sku.kind, SkuKind::Simple);
    }

    #[test]
    fn create_composite_sku() {
        let (mut store, _dir) = test_store();
        let part = store
            .create(CreateSku {
                sku_code: "PART-A".to_string(),
                name: "Part A".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Simple,
                active: Some(true),
                components: vec![],
            })
            .unwrap();
        let kit = store
            .create(CreateSku {
                sku_code: "KIT-01".to_string(),
                name: "Kit".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Composite,
                active: Some(true),
                components: vec![SkuComponent {
                    sku_id: part.id.clone(),
                    quantity: 2,
                }],
            })
            .unwrap();
        assert_eq!(kit.components.len(), 1);
        assert_eq!(kit.components[0].quantity, 2);
    }

    #[test]
    fn reject_composite_self_reference() {
        let (mut store, _dir) = test_store();
        let part = store
            .create(CreateSku {
                sku_code: "PART-B".to_string(),
                name: "Part B".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Simple,
                active: Some(true),
                components: vec![],
            })
            .unwrap();
        let kit = store
            .create(CreateSku {
                sku_code: "KIT-02".to_string(),
                name: "Kit 2".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Composite,
                active: Some(true),
                components: vec![SkuComponent {
                    sku_id: part.id,
                    quantity: 1,
                }],
            })
            .unwrap();
        let err = store
            .update(
                &kit.id,
                UpdateSku {
                    sku_code: kit.sku_code,
                    name: kit.name,
                    description: None,
                    category: None,
                    kind: SkuKind::Composite,
                    active: true,
                    components: vec![SkuComponent {
                        sku_id: kit.id.clone(),
                        quantity: 1,
                    }],
                },
            )
            .unwrap_err();
        assert!(matches!(err, StoreError::SelfReference));
    }
}
