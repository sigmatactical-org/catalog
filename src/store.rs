mod store_error;
pub use store_error::StoreError;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

#[derive(Debug, Clone)]
pub struct CatalogStore {
    pool: PgPool,
}

impl CatalogStore {
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect_as("catalog").await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let store = Self::connect().await?;
        sqlx::query("TRUNCATE catalog.sku_components, catalog.skus")
            .execute(&store.pool)
            .await?;
        Ok(store)
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list(&self) -> Result<Vec<Sku>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, sku_code, name, description, category, kind, active, updated_at \
             FROM catalog.skus ORDER BY lower(sku_code)",
        )
        .fetch_all(&self.pool)
        .await?;
        self.rows_to_skus(rows).await
    }

    pub async fn get(&self, id: &str) -> Result<Option<Sku>, StoreError> {
        let row = sqlx::query(
            "SELECT id, sku_code, name, description, category, kind, active, updated_at \
             FROM catalog.skus WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => {
                let skus = self.rows_to_skus(vec![row]).await?;
                Ok(skus.into_iter().next())
            }
            None => Ok(None),
        }
    }

    pub async fn create(&self, input: CreateSku) -> Result<Sku, StoreError> {
        self.validate_create(&input).await?;
        let sku = Sku::new(input);
        let mut tx = self.pool.begin().await?;
        insert_sku(&mut tx, &sku).await?;
        replace_components(&mut tx, &sku.id, &sku.components).await?;
        tx.commit().await?;
        Ok(sku)
    }

    pub async fn update(&self, id: &str, input: UpdateSku) -> Result<Sku, StoreError> {
        self.validate_update(id, &input).await?;
        let mut sku = self.get(id).await?.ok_or(StoreError::NotFound)?;
        sku.apply_update(input);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE catalog.skus SET sku_code = $2, name = $3, description = $4, category = $5, \
             kind = $6, active = $7, updated_at = $8 WHERE id = $1",
        )
        .bind(&sku.id)
        .bind(&sku.sku_code)
        .bind(&sku.name)
        .bind(&sku.description)
        .bind(&sku.category)
        .bind(kind_str(sku.kind))
        .bind(sku.active)
        .bind(parse_ts(&sku.updated_at)?)
        .execute(&mut *tx)
        .await?;
        replace_components(&mut tx, &sku.id, &sku.components).await?;
        tx.commit().await?;
        Ok(sku)
    }

    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        if self.get(id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let referencers: Vec<String> = sqlx::query_scalar(
            "SELECT s.sku_code FROM catalog.sku_components c \
             JOIN catalog.skus s ON s.id = c.parent_sku_id \
             WHERE c.component_sku_id = $1",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        if !referencers.is_empty() {
            return Err(StoreError::ReferencedByComposite(referencers.join(", ")));
        }
        sqlx::query("DELETE FROM catalog.skus WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn rows_to_skus(&self, rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<Sku>, StoreError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let comp_rows = sqlx::query(
            "SELECT parent_sku_id, component_sku_id, quantity FROM catalog.sku_components \
             WHERE parent_sku_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let mut components: HashMap<String, Vec<SkuComponent>> = HashMap::new();
        for row in comp_rows {
            let parent: String = row.get("parent_sku_id");
            components.entry(parent).or_default().push(SkuComponent {
                sku_id: row.get("component_sku_id"),
                quantity: row.get::<i32, _>("quantity") as u32,
            });
        }
        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                row_to_sku(row, components.remove(&id).unwrap_or_default())
            })
            .collect()
    }

    async fn validate_create(&self, input: &CreateSku) -> Result<(), StoreError> {
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, None).await? {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(None, input.kind, &input.components)
            .await
    }

    async fn validate_update(&self, id: &str, input: &UpdateSku) -> Result<(), StoreError> {
        if self.get(id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, Some(id)).await? {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(Some(id), input.kind, &input.components)
            .await
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

    async fn sku_code_exists(
        &self,
        sku_code: &str,
        except_id: Option<&str>,
    ) -> Result<bool, StoreError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM catalog.skus
                WHERE lower(sku_code) = lower($1)
                  AND ($2::text IS NULL OR id <> $2)
             )",
        )
        .bind(sku_code.trim())
        .bind(except_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn validate_components(
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
            let found: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM catalog.skus WHERE id = $1)")
                    .bind(&component.sku_id)
                    .fetch_one(&self.pool)
                    .await?;
            if !found {
                return Err(StoreError::ComponentNotFound(component.sku_id.clone()));
            }
        }
        if let Some(id) = self_id
            && self.would_cycle(id, components).await?
        {
            return Err(StoreError::CycleDetected);
        }
        Ok(())
    }

    async fn would_cycle(
        &self,
        root_id: &str,
        components: &[SkuComponent],
    ) -> Result<bool, StoreError> {
        let graph = self.component_graph().await?;
        let mut visited = HashSet::new();
        for component in components {
            if dfs_contains(&graph, &component.sku_id, root_id, &mut visited) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn component_graph(&self) -> Result<HashMap<String, Vec<String>>, StoreError> {
        let rows =
            sqlx::query("SELECT parent_sku_id, component_sku_id FROM catalog.sku_components")
                .fetch_all(&self.pool)
                .await?;
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            graph
                .entry(row.get("parent_sku_id"))
                .or_default()
                .push(row.get("component_sku_id"));
        }
        Ok(graph)
    }
}

async fn insert_sku(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sku: &Sku,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO catalog.skus (id, sku_code, name, description, category, kind, active, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&sku.id)
    .bind(&sku.sku_code)
    .bind(&sku.name)
    .bind(&sku.description)
    .bind(&sku.category)
    .bind(kind_str(sku.kind))
    .bind(sku.active)
    .bind(parse_ts(&sku.updated_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_components(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sku_id: &str,
    components: &[SkuComponent],
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM catalog.sku_components WHERE parent_sku_id = $1")
        .bind(sku_id)
        .execute(&mut **tx)
        .await?;
    for component in components {
        sqlx::query(
            "INSERT INTO catalog.sku_components (parent_sku_id, component_sku_id, quantity) \
             VALUES ($1, $2, $3)",
        )
        .bind(sku_id)
        .bind(&component.sku_id)
        .bind(component.quantity as i32)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn row_to_sku(
    row: sqlx::postgres::PgRow,
    components: Vec<SkuComponent>,
) -> Result<Sku, StoreError> {
    let kind_str: String = row.get("kind");
    Ok(Sku {
        id: row.get("id"),
        sku_code: row.get("sku_code"),
        name: row.get("name"),
        description: row.get("description"),
        category: row.get("category"),
        kind: parse_kind(&kind_str),
        active: row.get("active"),
        components,
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
}

fn kind_str(kind: SkuKind) -> &'static str {
    match kind {
        SkuKind::Simple => "simple",
        SkuKind::Composite => "composite",
    }
}

fn parse_kind(value: &str) -> SkuKind {
    match value {
        "composite" => SkuKind::Composite,
        _ => SkuKind::Simple,
    }
}

fn parse_ts(value: &str) -> Result<DateTime<Utc>, StoreError> {
    value
        .parse::<DateTime<Utc>>()
        .map_err(|e| StoreError::InvalidInput(format!("invalid timestamp: {e}")))
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
        let store = test_store().await;
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
        let store = test_store().await;
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
        let store = test_store().await;
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
