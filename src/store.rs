use std::collections::{HashMap, HashSet};

use sqlx::PgPool;
use thiserror::Error;

use crate::model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

const SCHEMA: &str = "catalog";

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
    #[error("database error: {0}")]
    Database(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Database {
    skus: Vec<Sku>,
}

#[derive(Debug, Clone)]
pub struct CatalogStore {
    pool: PgPool,
    db: Database,
}

impl CatalogStore {
    /// Connect to PostgreSQL and load the catalog snapshot.
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect().await?;
        let db: Database = sigma_pg::load_document(&pool, SCHEMA).await?;
        Ok(Self { pool, db })
    }

    /// Reset the catalog snapshot (tests only).
    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect().await?;
        let db = Database::default();
        sigma_pg::save_document(&pool, SCHEMA, &db).await?;
        Ok(Self { pool, db })
    }

    async fn persist(&self) -> Result<(), StoreError> {
        sigma_pg::save_document(&self.pool, SCHEMA, &self.db).await?;
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

    pub async fn create(&mut self, input: CreateSku) -> Result<Sku, StoreError> {
        self.validate_create(&input)?;
        let sku = Sku::new(input);
        self.db.skus.push(sku.clone());
        self.persist().await?;
        Ok(sku)
    }

    pub async fn update(&mut self, id: &str, input: UpdateSku) -> Result<Sku, StoreError> {
        self.validate_update(id, &input)?;
        let sku = self
            .db
            .skus
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;
        sku.apply_update(input);
        let updated = sku.clone();
        self.persist().await?;
        Ok(updated)
    }

    pub async fn delete(&mut self, id: &str) -> Result<(), StoreError> {
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
        self.persist().await
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

    async fn test_store() -> CatalogStore {
        CatalogStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    #[tokio::test]
    async fn create_simple_sku() {
        let mut store = test_store().await;
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
            .await
            .unwrap();
        assert_eq!(sku.sku_code, "WIDGET-01");
        assert_eq!(sku.kind, SkuKind::Simple);
    }

    #[tokio::test]
    async fn create_composite_sku() {
        let mut store = test_store().await;
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
            .await
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
            .await
            .unwrap();
        assert_eq!(kit.components.len(), 1);
        assert_eq!(kit.components[0].quantity, 2);
    }

    #[tokio::test]
    async fn reject_composite_self_reference() {
        let mut store = test_store().await;
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
            .await
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
            .await
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
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::SelfReference));
    }
}
